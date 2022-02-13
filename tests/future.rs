use futures_signals::cancelable_future;
use futures_util::future::{ready, FutureExt};
use std::task::Poll;

mod util;

#[test]
fn test_cancelable_future() {
    let mut a = cancelable_future(ready(()), || ());

    util::with_noop_context(|cx| {
        assert_eq!(a.1.poll_unpin(cx), Poll::Ready(()));
    });
}
