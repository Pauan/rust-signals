use std::rc::Rc;
use std::cell::Cell;
use std::task::Poll;
use futures_signals::cancelable_future;
use futures_signals::signal::{Signal, SignalExt, Mutable, Always, channel, always, option, result};
use futures_signals::signal_vec::VecDiff;
use futures_util::future::{ready, poll_fn};
use pin_utils::pin_mut;

mod util;


#[test]
fn test_always() {
    let mut signal = always(1);

    util::with_noop_context(|cx| {
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(Some(1)));
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(None));
    });
}


#[test]
fn test_option() {
    let mut signal = option::<Always<()>>(None);

    util::with_noop_context(|cx| {
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(Some(None)));
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(None));
    });
}

#[test]
fn test_option_signal() {
    let test_value = 0;

    let input = match test_value {
        0 => Some(util::Source::new(vec![
            Poll::Ready(0),
            Poll::Pending,
            Poll::Ready(3),
            Poll::Pending,
        ])),
        _ => None,
    };

    let output = option(input);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(Some(0))),
        Poll::Pending,
        Poll::Ready(Some(Some(3))),
        Poll::Pending,
        Poll::Ready(None),
    ]);
}


#[test]
fn test_result() {
    let mut signal = result::<Always<()>, _>(Err(5));

    util::with_noop_context(|cx| {
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(Some(Err(5))));
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(None));
    });
}

#[test]
fn test_result_signal() {
    let test_value = 0;

    let input = match test_value {
        0 => Ok(util::Source::new(vec![
            Poll::Ready(1),
            Poll::Pending,
            Poll::Ready(3),
            Poll::Pending,
        ])),
        _ => Err("hello"),
    };

    let output = result(input);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(Ok(1))),
        Poll::Pending,
        Poll::Ready(Some(Ok(3))),
        Poll::Pending,
        Poll::Ready(None),
    ]);
}


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
fn test_map() {
    let input = util::Source::new(vec![
        Poll::Ready(0),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(1),
        Poll::Ready(5),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(0),
        Poll::Pending,
        Poll::Ready(3),
        Poll::Pending,
    ]);

    let output = input.map(move |x| x * 10);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(0)),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(10)),
        Poll::Ready(Some(50)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(0)),
        Poll::Pending,
        Poll::Ready(Some(30)),
        Poll::Pending,
        Poll::Ready(None),
    ]);
}


#[test]
fn test_eq() {
    let input = util::Source::new(vec![
        Poll::Ready(0),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(1),
        Poll::Pending,
        Poll::Ready(5),
        Poll::Pending,
        Poll::Ready(3),
        Poll::Pending,
        Poll::Ready(1),
        Poll::Ready(2),
        Poll::Ready(3),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(1),
        Poll::Ready(1),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(0),
        Poll::Ready(1),
        Poll::Pending,
        Poll::Ready(3),
        Poll::Pending,
    ]);

    let output = input.eq(1);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(true)),
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(false)),
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Ready(None),
    ]);
}


#[test]
fn test_neq() {
    let input = util::Source::new(vec![
        Poll::Ready(0),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(1),
        Poll::Pending,
        Poll::Ready(5),
        Poll::Pending,
        Poll::Ready(3),
        Poll::Pending,
        Poll::Ready(1),
        Poll::Ready(2),
        Poll::Ready(3),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(1),
        Poll::Ready(1),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(0),
        Poll::Ready(1),
        Poll::Pending,
        Poll::Ready(3),
        Poll::Pending,
    ]);

    let output = input.neq(1);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(false)),
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(true)),
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Ready(None),
    ]);
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


#[test]
fn test_throttle() {
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

    let output = input.throttle(move || {
        let mut done = false;

        poll_fn(move |context| {
            if done {
                done = false;
                Poll::Ready(())

            } else {
                done = true;
                context.waker().wake_by_ref();
                Poll::Pending
            }
        })
    });

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(false)),
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(false)),
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Ready(None),
    ]);
}


#[test]
fn test_throttle_timing() {
    let input = util::Source::new(vec![
        Poll::Ready(0),
        Poll::Ready(1),
        Poll::Ready(2),
        Poll::Ready(3),
        Poll::Ready(4),
        Poll::Ready(5),
    ]);

    struct Called {
        ready: Mutable<bool>,
        function: Cell<u32>,
        future: Cell<u32>,
    }

    let called = Rc::new(Called {
        ready: Mutable::new(true),
        function: Cell::new(0),
        future: Cell::new(0),
    });

    let output = input.throttle({
        let called = called.clone();

        move || {
            called.function.set(called.function.get() + 1);

            let called = called.clone();

            async move {
                called.future.set(called.future.get() + 1);

                called.ready.signal().wait_for(true).await;
            }
        }
    });

    pin_mut!(output);

    util::with_noop_context(|cx| {
        assert_eq!(called.function.get(), 0);
        assert_eq!(called.future.get(), 0);

        assert_eq!(output.as_mut().poll_change(cx), Poll::Ready(Some(0)));
        assert_eq!(called.function.get(), 1);
        assert_eq!(called.future.get(), 1);

        assert_eq!(output.as_mut().poll_change(cx), Poll::Ready(Some(1)));
        assert_eq!(called.function.get(), 2);
        assert_eq!(called.future.get(), 2);

        called.ready.set(false);

        assert_eq!(output.as_mut().poll_change(cx), Poll::Ready(Some(2)));
        assert_eq!(called.function.get(), 3);
        assert_eq!(called.future.get(), 3);

        assert_eq!(output.as_mut().poll_change(cx), Poll::Pending);
        assert_eq!(called.function.get(), 3);
        assert_eq!(called.future.get(), 3);

        assert_eq!(output.as_mut().poll_change(cx), Poll::Pending);
        assert_eq!(called.function.get(), 3);
        assert_eq!(called.future.get(), 3);

        called.ready.set(true);

        assert_eq!(called.function.get(), 3);
        assert_eq!(called.future.get(), 3);

        assert_eq!(output.as_mut().poll_change(cx), Poll::Ready(Some(3)));
        assert_eq!(called.function.get(), 4);
        assert_eq!(called.future.get(), 4);

        assert_eq!(output.as_mut().poll_change(cx), Poll::Ready(Some(4)));
        assert_eq!(called.function.get(), 5);
        assert_eq!(called.future.get(), 5);

        assert_eq!(output.as_mut().poll_change(cx), Poll::Ready(Some(5)));
        assert_eq!(called.function.get(), 6);
        assert_eq!(called.future.get(), 6);

        assert_eq!(output.poll_change(cx), Poll::Ready(None));
    });
}
