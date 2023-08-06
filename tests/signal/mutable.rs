use std::task::Poll;
use futures_signals::cancelable_future;
use futures_signals::signal::{SignalExt, Mutable, channel};
use crate::util;


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
    let a = cancelable_future(async {}, || ());
    let _: Box<dyn Send + Sync> = Box::new(a.0);
    let _: Box<dyn Send + Sync> = Box::new(a.1);

    let _: Box<dyn Send + Sync> = Box::new(Mutable::new(1));
    let _: Box<dyn Send + Sync> = Box::new(Mutable::new(1).signal());
    let _: Box<dyn Send + Sync> = Box::new(Mutable::new(1).signal_cloned());

    let a = channel(1);
    let _: Box<dyn Send + Sync> = Box::new(a.0);
    let _: Box<dyn Send + Sync> = Box::new(a.1);
}

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

#[test]
fn is_from_t(){
    let src = 0;
    let _out: Mutable<u8> = Mutable::from(src);

    let src = 0;
    let _out: Mutable<u8> = src.into();
}