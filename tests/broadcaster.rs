extern crate pin_utils;
extern crate futures_core;
extern crate futures_util;
extern crate futures_executor;
#[macro_use]
extern crate futures_signals;

use futures_signals::signal::{SignalExt, Mutable, Broadcaster};
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

        drop(mutable);
        assert_eq!(b1.poll_change_unpin(cx), Poll::Ready(None));
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

    let polls = util::get_all_polls(map_ref!(signal1, signal2 => (*signal1, *signal2)), 0, |state, cx| {
        match *state {
            0 => {},
            1 => { cx.waker().wake_by_ref(); },
            2 => { mutable.as_ref().unwrap().set(5); },
            3 => { cx.waker().wake_by_ref(); },
            4 => { mutable.take(); },
            5 => { broadcaster.take(); },
            _ => {},
        }

        state + 1
    });

    assert_eq!(polls, vec![
        Poll::Ready(Some((1, 1))),
        Poll::Pending,
        Poll::Ready(Some((5, 5))),
        Poll::Pending,
        Poll::Ready(None),
    ]);
}
