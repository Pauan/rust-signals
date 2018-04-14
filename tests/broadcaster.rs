extern crate futures_core;
extern crate futures_executor;
extern crate futures_signals;

use futures_signals::signal::{Signal, Mutable, Broadcaster};
use futures_core::Async;

mod util;


#[test]
fn test_broadcaster() {
    let mutable = Mutable::new(1);
    let broadcaster = Broadcaster::new(mutable.signal());
    let mut b1 = broadcaster.signal();
    let mut b2 = broadcaster.signal_cloned();

    util::with_noop_context(|cx| {
        assert_eq!(b1.poll_change(cx), Async::Ready(Some(1)));
        assert_eq!(b1.poll_change(cx), Async::Pending);
        assert_eq!(b2.poll_change(cx), Async::Ready(Some(1)));
        assert_eq!(b2.poll_change(cx), Async::Pending);

        mutable.set(5);
        assert_eq!(b1.poll_change(cx), Async::Ready(Some(5)));
        assert_eq!(b1.poll_change(cx), Async::Pending);
        assert_eq!(b2.poll_change(cx), Async::Ready(Some(5)));
        assert_eq!(b2.poll_change(cx), Async::Pending);

        drop(mutable);
        assert_eq!(b1.poll_change(cx), Async::Ready(None));
        assert_eq!(b2.poll_change(cx), Async::Ready(None));
    });
}
