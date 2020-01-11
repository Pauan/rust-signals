use std::rc::Rc;
use std::cell::Cell;
use std::task::Poll;
use futures_signals::cancelable_future;
use futures_signals::signal::{SignalExt, Mutable, channel};
use futures_signals::signal_vec::VecDiff;
use futures_util::future::{ready, poll_fn};

mod util;


#[test]
fn test_mutable() {
    let mutable = Mutable::new(1);
    let mut s1 = mutable.signal();
    let mut s2 = mutable.signal_cloned();

    util::with_noop_context(|cx| {
        assert_eq!(s1.poll_change_unpin(cx), Poll::Ready(Some(1)));
        assert_eq!(s1.poll_change_unpin(cx), Poll::Pending);
        assert_eq!(s2.poll_change_unpin(cx), Poll::Ready(Some(1)));
        assert_eq!(s2.poll_change_unpin(cx), Poll::Pending);

        mutable.set(5);
        assert_eq!(s1.poll_change_unpin(cx), Poll::Ready(Some(5)));
        assert_eq!(s1.poll_change_unpin(cx), Poll::Pending);
        assert_eq!(s2.poll_change_unpin(cx), Poll::Ready(Some(5)));
        assert_eq!(s2.poll_change_unpin(cx), Poll::Pending);

        drop(mutable);
        assert_eq!(s1.poll_change_unpin(cx), Poll::Ready(None));
        assert_eq!(s2.poll_change_unpin(cx), Poll::Ready(None));
    });
}

#[test]
fn test_mutable_drop() {
    {
        let mutable = Mutable::new(1);
        let mut s1 = mutable.signal();
        let mut s2 = mutable.signal_cloned();
        drop(mutable);

        util::with_noop_context(|cx| {
            assert_eq!(s1.poll_change_unpin(cx), Poll::Ready(Some(1)));
            assert_eq!(s1.poll_change_unpin(cx), Poll::Ready(None));
            assert_eq!(s2.poll_change_unpin(cx), Poll::Ready(Some(1)));
            assert_eq!(s2.poll_change_unpin(cx), Poll::Ready(None));
        });
    }

    {
        let mutable = Mutable::new(1);
        let mut s1 = mutable.signal();
        let mut s2 = mutable.signal_cloned();

        util::with_noop_context(|cx| {
            assert_eq!(s1.poll_change_unpin(cx), Poll::Ready(Some(1)));
            assert_eq!(s1.poll_change_unpin(cx), Poll::Pending);
            assert_eq!(s2.poll_change_unpin(cx), Poll::Ready(Some(1)));
            assert_eq!(s2.poll_change_unpin(cx), Poll::Pending);

            mutable.set(5);
            drop(mutable);

            assert_eq!(s1.poll_change_unpin(cx), Poll::Ready(Some(5)));
            assert_eq!(s1.poll_change_unpin(cx), Poll::Ready(None));
            assert_eq!(s2.poll_change_unpin(cx), Poll::Ready(Some(5)));
            assert_eq!(s2.poll_change_unpin(cx), Poll::Ready(None));
        });
    }

    {
        let mutable = Mutable::new(1);
        let mut s1 = mutable.signal();
        let mut s2 = mutable.signal_cloned();

        util::with_noop_context(|cx| {
            assert_eq!(s1.poll_change_unpin(cx), Poll::Ready(Some(1)));
            assert_eq!(s1.poll_change_unpin(cx), Poll::Pending);
            assert_eq!(s2.poll_change_unpin(cx), Poll::Ready(Some(1)));
            assert_eq!(s2.poll_change_unpin(cx), Poll::Pending);

            mutable.set(5);
            assert_eq!(s1.poll_change_unpin(cx), Poll::Ready(Some(5)));
            assert_eq!(s1.poll_change_unpin(cx), Poll::Pending);

            drop(mutable);
            assert_eq!(s2.poll_change_unpin(cx), Poll::Ready(Some(5)));
            assert_eq!(s2.poll_change_unpin(cx), Poll::Ready(None));

            assert_eq!(s1.poll_change_unpin(cx), Poll::Ready(None));
        });
    }
}

#[test]
fn test_send_sync() {
    let a = cancelable_future(ready(()), || ());
    let _: Box<dyn Send + Sync> = Box::new(a.0);
    let _: Box<dyn Send + Sync> = Box::new(a.1);

    let _: Box<dyn Send + Sync> = Box::new(Mutable::new(1));
    let _: Box<dyn Send + Sync> = Box::new(Mutable::new(1).signal());
    let _: Box<dyn Send + Sync> = Box::new(Mutable::new(1).signal_cloned());

    let a = channel(1);
    let _: Box<dyn Send + Sync> = Box::new(a.0);
    let _: Box<dyn Send + Sync> = Box::new(a.1);
}


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


#[test]
fn test_switch_signal_vec() {
    let input = util::Source::new(vec![
        Poll::Ready(true),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(false),
        Poll::Ready(false),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(false),
        Poll::Ready(true),
        Poll::Pending,
    ]);

    let output = input.switch_signal_vec(move |test| {
        if test {
            util::Source::new(vec![
                Poll::Ready(VecDiff::Push { value: 10 }),
            ])

        } else {
            util::Source::new(vec![
                Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
                Poll::Ready(VecDiff::Push { value: 6 }),
                Poll::Pending,
                Poll::Pending,
                Poll::Ready(VecDiff::InsertAt { index: 0, value: 7 }),
            ])
        }
    });

    util::assert_signal_vec_eq(output, vec![
        Poll::Ready(Some(VecDiff::Push { value: 10 })),
        Poll::Pending,
        Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
        Poll::Ready(Some(VecDiff::Push { value: 6 })),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(VecDiff::InsertAt { index: 0, value: 7 })),
        Poll::Pending,
        Poll::Ready(Some(VecDiff::Replace { values: vec![] })),
        Poll::Ready(Some(VecDiff::Push { value: 10 })),
        Poll::Ready(None)
    ]);
}
