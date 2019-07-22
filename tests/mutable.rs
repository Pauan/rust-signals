use std::task::Poll;
use futures_signals::signal::Mutable;

mod util;


// Verifies that lock_mut only notifies when it is mutated
#[test]
fn test_lock_mut() {
    {
        let mut mutable = Some(Mutable::new(1));

        let polls = util::get_all_polls(mutable.as_ref().unwrap().signal(), 0, move |state, _cx| {
            match *state {
                0 => {},
                _ => {
                    {
                        let mut lock = mutable.as_ref().unwrap().lock_mut();

                        if *lock == 2 {
                            *lock = 5;
                        }
                    }

                    mutable.take();
                },
            }

            state + 1
        });

        assert_eq!(polls, vec![
            Poll::Ready(Some(1)),
            Poll::Ready(None),
        ]);
    }

    {
        let mut mutable = Some(Mutable::new(1));

        let polls = util::get_all_polls(mutable.as_ref().unwrap().signal(), 0, move |state, _cx| {
            match *state {
                0 => {},
                1 => {
                    {
                        let mut lock = mutable.as_ref().unwrap().lock_mut();

                        if *lock == 1 {
                            *lock = 5;
                        }
                    }

                    mutable.take();
                },
                _ => {},
            }

            state + 1
        });

        assert_eq!(polls, vec![
            Poll::Ready(Some(1)),
            Poll::Ready(Some(5)),
            Poll::Ready(None),
        ]);
    }
}
