//! Explicit native-ABI acceptance harness: one running place and one queued place.
//! Run with a fresh, caller-supplied fleet; no listener or Claviron server is booted.
use claviron::orbit::link::application::{
    ApplicationClient, ApplicationReceiver, ApplicationSegment, ControlCommand,
    Exchange, FrameKind, Received, frame_kind,
};
use claviron_orbit::*;

#[track_caller]
fn check(status: u32) {
    assert_eq!(status, Status::Ok as u32);
}

async fn event(app: u64) -> Event {
    tokio::task::spawn_blocking(move || {
        let mut event = Event::default();
        check(unsafe { orbit_wait(app, &mut event) });
        event
    })
    .await
    .unwrap()
}

async fn cancel_to_completion(
    app: u64,
    exchange: u64,
) {
    check(orbit_exchange_cancel(exchange));
    loop {
        let event = event(app).await;
        assert_eq!(event.exchange, exchange);
        if event.kind == EventKind::Complete as u32 {
            break;
        }
        assert_eq!(event.kind, EventKind::Cancelled as u32);
    }
}

async fn control_barrier(
    client: &ApplicationClient,
    command: &ControlCommand,
    receiver: &mut ApplicationReceiver,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    client.command(command)?;
    let mut bytes = Vec::new();
    loop {
        if matches!(receiver.next(&mut bytes).await?, Received::Frame)
            && matches!(frame_kind(&bytes), Ok(FrameKind::ApplicationState))
        {
            return Ok(());
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fleet = std::env::args().nth(1).ok_or("expected a fresh FLEET name")?;
    let mut runtime = 0;
    check(unsafe {
        orbit_runtime_open(
            &RuntimeOptions {
                abi_version: ABI_VERSION,
                execution: 1,
                native_threads: 1,
            },
            &mut runtime,
        )
    });
    let mut app = 0;
    check(unsafe {
        orbit_open(
            runtime,
            fleet.as_ptr(),
            fleet.len() as u64,
            b"example".as_ptr(),
            7,
            &OpenOptions {
                abi_version: ABI_VERSION,
                incarnation: 1,
                max_in_flight: 1,
                max_queued: 1,
                max_request_bytes: 0,
                max_response_bytes: 0,
                io_buffer_bytes: 1024,
            },
            &mut app,
        )
    });
    let client = ApplicationClient::join(&fleet, "caller", 2)?;
    let segment = std::sync::Arc::new(
        ApplicationSegment::existing_for_fleet(&fleet)?.unwrap(),
    );
    let observer = segment.join("observer", "edge", 3)?;
    let mut receiver = segment.application_receiver(observer.lane)?;
    let barrier = ControlCommand {
        application: "example".into(),
        name: "status".into(),
        origin: Some(observer.lane as u16),
        target: None,
        payload: Vec::new(),
    };
    let request = || Exchange::default();
    let first = client.send("example", request())?;
    let started = event(app).await;
    assert_eq!(started.kind, EventKind::Request as u32);
    let queued = client.send("example", request())?;
    assert!(matches!(
        client.send("example", request()),
        Err(claviron::orbit::link::application::Error::CapacityFull)
    ));
    // The reply is an ordering barrier: the native reader has passed the
    // queued request and still processes control frames while execution is full.
    control_barrier(&client, &barrier, &mut receiver).await?;
    cancel_to_completion(app, started.exchange).await;
    control_barrier(&client, &barrier, &mut receiver).await?;
    // Completion alone must not promote waiting work before the C handle
    // is released. This queued ticket must still be withdrawable.
    let (_, _, reply, _) = queued.split();
    assert!(reply.withdrawal().unwrap().withdraw());
    drop(reply);
    let replacement = client.send("example", request())?;
    check(orbit_exchange_release(app, started.exchange));
    drop(first);
    // Releasing the foreign handle must wake native intake and pass the
    // withdrawn ticket without a timer or another request being needed.
    let promoted = event(app).await;
    assert_eq!(promoted.kind, EventKind::Request as u32);
    assert_ne!(promoted.exchange, started.exchange);
    cancel_to_completion(app, promoted.exchange).await;
    check(orbit_exchange_release(app, promoted.exchange));
    drop(replacement);
    check(orbit_withdraw(app));
    assert_eq!(event(app).await.kind, EventKind::Closed as u32);
    check(orbit_release(app));
    tokio::task::spawn_blocking(move || check(orbit_runtime_release(runtime)))
        .await?;
    client.leave();
    drop(client);
    drop(receiver);
    segment.inbox().release_lane(observer.lane)?;
    drop(observer);
    segment.unlink()?;
    println!(
        "Native admission: running/queued bounds, control intake, withdrawal and release wake passed"
    );
    Ok(())
}
