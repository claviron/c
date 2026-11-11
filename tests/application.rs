use super::*;

#[test]
fn options_require_declared_budgets_and_identity() {
    let mut options = Options {
        fleet: "example".into(),
        name: "app".into(),
        incarnation: 1,
        max_in_flight: 2,
        max_queued: 0,
        max_request_bytes: 128,
        max_response_bytes: 128,
        io_buffer_bytes: 16,
    };
    assert_eq!(options.validate(), Ok(2));
    options.max_queued = claviron::orbit::link::application::LINK_INBOX_CAPACITY
        - 1 - options.max_in_flight;
    assert_eq!(
        options.validate(),
        Ok(claviron::orbit::link::application::LINK_INBOX_CAPACITY - 1)
    );
    options.max_queued += 1;
    assert_eq!(options.validate(), Err(Status::InvalidArgument));
    options.max_queued = usize::MAX;
    assert_eq!(options.validate(), Err(Status::InvalidArgument));
    options.max_queued = 2;
    options.incarnation = 0;
    assert_eq!(options.validate(), Err(Status::InvalidArgument));
}

#[test]
fn streaming_requires_a_window_but_not_a_total_body_limit() {
    let mut options = Options {
        fleet: "example".into(),
        name: "app".into(),
        incarnation: 1,
        max_in_flight: 2,
        max_queued: 0,
        max_request_bytes: 0,
        max_response_bytes: 0,
        io_buffer_bytes: 4096,
    };
    assert_eq!(options.validate(), Ok(2));
    options.io_buffer_bytes = 0;
    assert_eq!(options.validate(), Err(Status::InvalidArgument));
}

#[test]
fn queue_counts_only_waiting_exchanges() {
    let mut options = Options {
        fleet: "example".into(),
        name: "app".into(),
        incarnation: 1,
        max_in_flight: 2,
        max_queued: 1,
        max_request_bytes: 0,
        max_response_bytes: 0,
        io_buffer_bytes: 16,
    };
    assert_eq!(options.validate(), Ok(3));
    options.max_queued = 0;
    assert_eq!(options.validate(), Ok(2));
    options.max_in_flight = 0;
    assert_eq!(options.validate(), Err(Status::InvalidArgument));
}

#[test]
fn releases_coalesce_without_losing_work_or_the_next_wake() {
    use std::future::Future;
    use std::sync::atomic::AtomicUsize;
    use std::task::{Context, Poll, Wake, Waker};

    #[derive(Default)]
    struct Wakes(AtomicUsize);
    impl Wake for Wakes {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    let releases = Releases::default();
    let wakes = Arc::new(Wakes::default());
    let waker = Waker::from(Arc::clone(&wakes));
    let mut context = Context::from_waker(&waker);
    let mut waiting = std::pin::pin!(releases.changed.notified());
    assert_eq!(waiting.as_mut().poll(&mut context), Poll::Pending);
    releases.record();
    releases.record();
    assert_eq!(wakes.0.load(Ordering::Relaxed), 1);
    assert_eq!(waiting.as_mut().poll(&mut context), Poll::Ready(()));
    assert_eq!(releases.take(), 2);
    assert_eq!(releases.take(), 0);
    // A release before the next waiter registers must leave a permit too.
    releases.record();
    let mut next = std::pin::pin!(releases.changed.notified());
    assert_eq!(next.as_mut().poll(&mut context), Poll::Ready(()));
    assert_eq!(releases.take(), 1);
}
