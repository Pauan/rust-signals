extern crate futures_core;
extern crate futures_executor;
extern crate futures_signals;

use futures_signals::cancelable_future;
use futures_signals::signal::{Signal, Mutable, channel};
use futures_core::{Async, Future};
use futures_core::future::{FutureResult};

mod util;


#[test]
fn test_mutable() {
    let mutable = Mutable::new(1);
    let mut s = mutable.signal();

    util::with_noop_context(|cx| {
        assert_eq!(s.poll(cx), Async::Ready(Some(1)));
        assert_eq!(s.poll(cx), Async::Pending);

        mutable.set(5);
        assert_eq!(s.poll(cx), Async::Ready(Some(5)));
        assert_eq!(s.poll(cx), Async::Pending);
    });
}

#[test]
fn test_cancelable_future() {
    let mut a = cancelable_future(Ok(()), |_: FutureResult<(), ()>| ());

    util::with_noop_context(|cx| {
        assert_eq!(a.1.poll(cx), Ok(Async::Ready(())));
    });
}

#[test]
fn test_send_sync() {
    let a = cancelable_future(Ok(()), |_: FutureResult<(), ()>| ());
    let _: Box<Send + Sync> = Box::new(a.0);
    let _: Box<Send + Sync> = Box::new(a.1);

    let _: Box<Send + Sync> = Box::new(Mutable::new(1));
    let _: Box<Send + Sync> = Box::new(Mutable::new(1).signal());
    let _: Box<Send + Sync> = Box::new(Mutable::new(1).signal_cloned());

    let a = channel(1);
    let _: Box<Send + Sync> = Box::new(a.0);
    let _: Box<Send + Sync> = Box::new(a.1);
}
