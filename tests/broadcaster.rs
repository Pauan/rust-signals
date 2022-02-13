use futures_executor::block_on_stream;
use futures_signals::map_ref;
use futures_signals::signal::{always, from_stream, Broadcaster, Mutable, SignalExt};
use std::task::Poll;

mod util;

#[test]
fn test_broadcaster() {
    let mutable = Mutable::new(1);
    let broadcaster = Broadcaster::new(mutable.signal());
    let mut b1 = broadcaster.signal();
    let mut b2 = broadcaster.signal_cloned();

    util::with_noop_context(|cx| {
        assert_eq!(b1.poll_change_unpin(cx), Poll::Ready(Some(1)));
        assert_eq!(b1.poll_change_unpin(cx), Poll::Pending);
        assert_eq!(b2.poll_change_unpin(cx), Poll::Ready(Some(1)));
        assert_eq!(b2.poll_change_unpin(cx), Poll::Pending);

        mutable.set(5);
        assert_eq!(b1.poll_change_unpin(cx), Poll::Ready(Some(5)));
        assert_eq!(b1.poll_change_unpin(cx), Poll::Pending);
        assert_eq!(b2.poll_change_unpin(cx), Poll::Ready(Some(5)));
        assert_eq!(b2.poll_change_unpin(cx), Poll::Pending);

        mutable.set(6);
        drop(mutable);
        assert_eq!(b1.poll_change_unpin(cx), Poll::Ready(Some(6)));
        assert_eq!(b1.poll_change_unpin(cx), Poll::Ready(None));
        assert_eq!(b2.poll_change_unpin(cx), Poll::Ready(Some(6)));
        assert_eq!(b2.poll_change_unpin(cx), Poll::Ready(None));
    });
}

#[test]
fn test_polls() {
    let mutable = Mutable::new(1);
    let broadcaster = Broadcaster::new(mutable.signal());
    let signal1 = broadcaster.signal();
    let signal2 = broadcaster.signal();

    let mut mutable = Some(mutable);
    let mut broadcaster = Some(broadcaster);

    let polls = util::get_all_polls(
        map_ref!(signal1, signal2 => (*signal1, *signal2)),
        0,
        |state, cx| {
            match *state {
                0 => {}
                1 => {
                    cx.waker().wake_by_ref();
                }
                2 => {
                    mutable.as_ref().unwrap().set(5);
                }
                3 => {
                    cx.waker().wake_by_ref();
                }
                4 => {
                    mutable.take();
                }
                5 => {
                    broadcaster.take();
                }
                _ => {}
            }

            state + 1
        },
    );

    assert_eq!(
        polls,
        vec![
            Poll::Ready(Some((1, 1))),
            Poll::Pending,
            Poll::Ready(Some((5, 5))),
            Poll::Pending,
            Poll::Ready(None),
        ]
    );
}

#[test]
fn test_broadcaster_signal_ref() {
    let broadcaster = Broadcaster::new(always(1));
    let mut signal = broadcaster.signal_ref(|x| x + 5);
    util::with_noop_context(|cx| {
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(Some(6)));
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(None));
    });
}

#[test]
fn test_broadcaster_always() {
    let broadcaster = Broadcaster::new(always(1));
    let mut signal = broadcaster.signal();
    util::with_noop_context(|cx| {
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(Some(1)));
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(None));
    });
}

#[test]
fn test_broadcaster_drop() {
    let mutable = Mutable::new(1);
    let broadcaster = Broadcaster::new(mutable.signal());
    let mut signal = broadcaster.signal();
    util::with_noop_context(|cx| {
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(Some(1)));
        drop(mutable);
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(None));
    });
}

#[test]
fn test_broadcaster_multiple() {
    let mutable = Mutable::new(1);
    let broadcaster = Broadcaster::new(mutable.signal());
    let mut signal1 = broadcaster.signal();
    let mut signal2 = broadcaster.signal();
    drop(mutable);
    util::with_noop_context(|cx| {
        assert_eq!(signal1.poll_change_unpin(cx), Poll::Ready(Some(1)));
        assert_eq!(signal1.poll_change_unpin(cx), Poll::Ready(None));
        assert_eq!(signal2.poll_change_unpin(cx), Poll::Ready(Some(1)));
        assert_eq!(signal2.poll_change_unpin(cx), Poll::Ready(None));
    });
}

#[test]
fn test_block_on_stream() {
    let observable = Mutable::new(1);
    let signal_from_stream = observable.signal();

    let broadcaster = Broadcaster::new(signal_from_stream);
    let mut blocking_stream = block_on_stream(broadcaster.signal().to_stream());

    assert_eq!(blocking_stream.next(), Some(1));
    observable.set(2);

    assert_eq!(blocking_stream.next(), Some(2));
}

#[test]
fn test_block_on_stream_wrapper() {
    let observable = Mutable::new(1);
    let signal_from_stream = from_stream(observable.signal().to_stream());

    let broadcaster = Broadcaster::new(signal_from_stream);
    let mut blocking_stream = block_on_stream(broadcaster.signal().debug().to_stream());

    assert_eq!(blocking_stream.next().unwrap(), Some(1));
    observable.set(2);

    assert_eq!(blocking_stream.next().unwrap(), Some(2));
}
