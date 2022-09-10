use std::rc::Rc;
use std::cell::Cell;
use std::task::Poll;
use futures_signals::signal::{Signal, SignalExt, Mutable};
use futures_util::future::poll_fn;
use pin_utils::pin_mut;
use crate::util;


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
