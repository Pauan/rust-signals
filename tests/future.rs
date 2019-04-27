extern crate pin_utils;
extern crate futures_core;
extern crate futures_util;
extern crate futures_executor;
extern crate futures_signals;

use std::task::Poll;
use futures_signals::cancelable_future;
use futures_util::future::{ready, FutureExt};

mod util;


#[test]
fn test_cancelable_future() {
    let mut a = cancelable_future(ready(()), || ());

    util::with_noop_context(|cx| {
        assert_eq!(a.1.poll_unpin(cx), Poll::Ready(()));
    });
}
