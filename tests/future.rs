#![feature(pin, futures_api)]

extern crate pin_utils;
extern crate futures_core;
extern crate futures_util;
extern crate futures_executor;
extern crate futures_signals;

use futures_signals::cancelable_future;
use futures_core::Poll;
use futures_util::future::{ready, FutureExt};

mod util;


#[test]
fn test_cancelable_future() {
    let mut a = cancelable_future(ready(()), || ());

    util::with_noop_waker(|waker| {
        assert_eq!(a.1.poll_unpin(waker), Poll::Ready(()));
    });
}
