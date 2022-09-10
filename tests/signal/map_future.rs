use std::rc::Rc;
use std::cell::Cell;
use std::task::Poll;
use futures_signals::signal::{SignalExt, Mutable};
use futures_util::future::poll_fn;
use crate::util;


#[test]
fn test_map_future() {
    let mutable = Rc::new(Mutable::new(1));

    let first = Rc::new(Cell::new(true));

    let s = {
        let first = first.clone();

        mutable.signal().map_future(move |value| {
            let first = first.clone();

            poll_fn(move |_| {
                if first.get() {
                    Poll::Pending

                } else {
                    Poll::Ready(value)
                }
            })
        })
    };

    util::ForEachSignal::new(s)
        .next({
            let mutable = mutable.clone();
            move |_, change| {
                assert_eq!(change, Poll::Ready(Some(None)));
                mutable.set(2);
            }
        })
        .next({
            let mutable = mutable.clone();
            move |_, change| {
                assert_eq!(change, Poll::Pending);
                first.set(false);
                mutable.set(3);
            }
        })
        .next(|_, change| {
            assert_eq!(change, Poll::Ready(Some(Some(3))));
        })
        .run();
}
