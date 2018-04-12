extern crate futures_core;
extern crate futures_executor;
extern crate futures_signals;

use futures_signals::cancelable_future;
use futures_core::{Async, Future};
use futures_core::future::{FutureResult};

mod util;


#[test]
fn test_cancelable_future() {
    let mut a = cancelable_future(Ok(()), |_: FutureResult<(), ()>| ());

    util::with_noop_context(|cx| {
        assert_eq!(a.1.poll(cx), Ok(Async::Ready(())));
    });
}
