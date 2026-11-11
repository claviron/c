use super::*;
use std::sync::atomic::AtomicUsize;

unsafe extern "C" fn wake(context: *mut c_void) {
    let count = unsafe { &*context.cast::<AtomicUsize>() };
    count.fetch_add(1, Ordering::SeqCst);
}

fn event() -> Event {
    Event { kind: 1, status: 0, exchange: 17 }
}

#[test]
fn attachment_wakes_for_preexisting_events_and_detach_retires_context() {
    let events = Completions::new(2);
    let count = AtomicUsize::new(0);
    events.push(event());
    assert!(events.try_event().unwrap().is_some());
    assert!(events.try_event().unwrap().is_none());
    events.push(event());
    let callback = Wake {
        callback: wake,
        context: (&count as *const AtomicUsize).cast_mut().cast(),
    };
    events.attach(callback).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(events.attach(callback), Err(Status::Busy));
    assert_eq!(events.try_event().unwrap().unwrap().exchange, 17);
    events.detach();
    events.push(event());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(events.try_event().unwrap().is_some());
}

#[test]
fn concurrent_producers_keep_all_completions_when_host_wakes_coalesce() {
    let events = Completions::new(64);
    let count = AtomicUsize::new(0);
    events
        .attach(Wake {
            callback: wake,
            context: (&count as *const AtomicUsize).cast_mut().cast(),
        })
        .unwrap();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let events = &events;
            scope.spawn(move || {
                for _ in 0..16 {
                    events.push(event());
                }
            });
        }
    });
    // A host may observe only one wake; it still drains the complete queue.
    let mut delivered = 0;
    while events.try_event().unwrap().is_some() {
        delivered += 1;
    }
    assert_eq!(delivered, 64);
    assert_eq!(count.load(Ordering::SeqCst), 65);
    events.detach();
}

#[test]
fn detach_waits_for_an_inflight_wake_before_releasing_its_context() {
    struct Context {
        entered: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }
    unsafe extern "C" fn held_wake(context: *mut c_void) {
        let context = unsafe { &*context.cast::<Context>() };
        context.entered.send(()).unwrap();
        context.release.lock().unwrap().recv().unwrap();
    }
    let events = Completions::new(1);
    let (entered, arrived) = mpsc::channel();
    let (release, resumed) = mpsc::channel();
    let context = Context { entered, release: Mutex::new(resumed) };
    // Install without invoking: the next push is the callback being held.
    *events.wake.lock().unwrap() = Some(Wake {
        callback: held_wake,
        context: (&context as *const Context).cast_mut().cast(),
    });
    events.attached.store(true, Ordering::Release);
    std::thread::scope(|scope| {
        scope.spawn(|| events.push(event()));
        arrived.recv().unwrap();
        let detached = scope.spawn(|| events.detach());
        release.send(()).unwrap();
        detached.join().unwrap();
    });
    assert!(events.wake.lock().unwrap().is_none());
    events.signal();
    assert!(arrived.try_recv().is_err());
}
