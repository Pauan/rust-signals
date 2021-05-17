use std::task::Poll;
use futures_signals::signal::Mutable;

mod util;


// Verifies that lock_mut only notifies when it is mutated
#[test]
fn test_lock_mut() {
    {
        let m = Mutable::new(1);

        let polls = util::get_signal_polls(m.signal(), move || {
            let mut lock = m.lock_mut();

            if *lock == 2 {
                *lock = 5;
            }
        });

        assert_eq!(polls, vec![
            Poll::Ready(Some(1)),
            Poll::Pending,
            Poll::Ready(None),
        ]);
    }

    {
        let m = Mutable::new(1);

        let polls = util::get_signal_polls(m.signal(), move || {
            let mut lock = m.lock_mut();

            if *lock == 1 {
                *lock = 5;
            }
        });

        assert_eq!(polls, vec![
            Poll::Ready(Some(1)),
            Poll::Pending,
            Poll::Ready(Some(5)),
            Poll::Ready(None),
        ]);
    }
}


/*#[test]
fn test_lock_panic() {
    struct Foo;

    impl Foo {
        fn bar<A>(self, _value: &A) -> Self {
            self
        }

        fn qux<A>(self, _value: A) -> Self {
            self
        }
    }

    let m = Mutable::new(1);

    Foo
        .bar(&m.lock_ref())
        .qux(m.signal().map(move |x| x * 10));
}


#[test]
fn test_lock_mut_signal() {
    let m = Mutable::new(1);

    let mut output = {
        let mut lock = m.lock_mut();
        let output = lock.signal().map(move |x| x * 10);
        *lock = 2;
        output
    };

    util::with_noop_context(|cx| {
        assert_eq!(output.poll_change_unpin(cx), Poll::Ready(Some(20)));
        assert_eq!(output.poll_change_unpin(cx), Poll::Pending);
        assert_eq!(output.poll_change_unpin(cx), Poll::Pending);

        m.set(5);

        assert_eq!(output.poll_change_unpin(cx), Poll::Ready(Some(50)));
        assert_eq!(output.poll_change_unpin(cx), Poll::Pending);
        assert_eq!(output.poll_change_unpin(cx), Poll::Pending);

        drop(m);

        assert_eq!(output.poll_change_unpin(cx), Poll::Ready(None));
        assert_eq!(output.poll_change_unpin(cx), Poll::Ready(None));
    });
}*/
