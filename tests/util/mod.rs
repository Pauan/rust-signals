use std::pin::Unpin;
use futures_signals::signal_vec::{VecDiff, SignalVec};
use futures_signals::signal::Signal;
use futures_core::Poll;
use futures_core::task::{LocalWaker, Wake, local_waker_from_nonlocal};
use futures_util::future::poll_fn;
use futures_executor::block_on;
use pin_utils::pin_mut;
use std::sync::Arc;
use std::pin::Pin;


#[allow(dead_code)]
pub struct ForEachSignal<A> where A: Signal {
    signal: A,
    callbacks: Vec<Box<FnMut(&LocalWaker, Poll<Option<A::Item>>)>>,
}

#[allow(dead_code)]
impl<A> ForEachSignal<A> where A: Signal {
    pub fn new(signal: A) -> Self {
        Self {
            signal,
            callbacks: vec![],
        }
    }

    pub fn next<B>(mut self, callback: B) -> Self where B: FnMut(&LocalWaker, Poll<Option<A::Item>>) + 'static {
        self.callbacks.insert(0, Box::new(callback));
        self
    }

    pub fn run(self) {
        let mut callbacks = self.callbacks;
        let mut signal = self.signal;

        block_on(poll_fn(move |waker| -> Poll<()> {
            loop {
                return match callbacks.pop() {
                    Some(mut callback) => {
                        // TODO is this safe ?
                        let poll = unsafe { Pin::new_unchecked(&mut signal) }.poll_change(waker);

                        match poll {
                            Poll::Ready(None) => {
                                callback(waker, poll);
                                Poll::Ready(())
                            },
                            Poll::Ready(Some(_)) => {
                                callback(waker, poll);
                                continue;
                            },
                            Poll::Pending => {
                                callback(waker, poll);
                                Poll::Pending
                            },
                        }
                    },
                    None => {
                        Poll::Ready(())
                    },
                }
            }
        }));
    }
}



#[allow(dead_code)]
pub fn with_noop_waker<U, F: FnOnce(&LocalWaker) -> U>(f: F) -> U {
    // borrowed this design from the futures source
    struct Noop;

    impl Wake for Noop {
        fn wake(_: &Arc<Self>) {}
    }

    // TODO is this correct ?
    let waker = local_waker_from_nonlocal(Arc::new(Noop));

    f(&waker)
}


#[allow(dead_code)]
pub fn get_all_polls<A, B, F>(signal: A, mut initial: B, mut f: F) -> Vec<Poll<Option<A::Item>>> where A: Signal, F: FnMut(&B, &LocalWaker) -> B {
    let mut output = vec![];

    // TODO is this correct ?
    pin_mut!(signal);

    block_on(poll_fn(|waker| {
        loop {
            initial = f(&initial, waker);

            // TODO is this correct ?
            let x = Pin::as_mut(&mut signal).poll_change(waker);

            let x: Poll<()> = match x {
                Poll::Ready(Some(_)) => {
                    output.push(x);
                    continue;
                },
                Poll::Ready(None) => {
                    output.push(x);
                    Poll::Ready(())
                },
                Poll::Pending => {
                    output.push(x);
                    Poll::Pending
                },
            };

            return x;
        }
    }));

    output
}


#[allow(dead_code)]
pub fn map_poll_vec<A, B, C>(signal: A, mut callback: C) -> Vec<B> where A: SignalVec, C: FnMut(&A, Poll<Option<VecDiff<A::Item>>>) -> B {
    let mut changes = vec![];

    // TODO is this correct ?
    pin_mut!(signal);

    block_on(poll_fn(|waker| {
        loop {
            // TODO is this correct ?
            let x = Pin::as_mut(&mut signal).poll_vec_change(waker);

            return match x {
                Poll::Ready(Some(_)) => {
                    changes.push(callback(&signal, x));
                    continue;
                },
                Poll::Ready(None) => {
                    changes.push(callback(&signal, x));
                    Poll::Ready(())
                },
                Poll::Pending => {
                    changes.push(callback(&signal, x));
                    Poll::Pending
                },
            };
        }
    }));

    changes
}


#[allow(dead_code)]
pub fn assert_signal_eq<A, S>(signal: S, expected: Vec<Poll<Option<A>>>)
    where A: std::fmt::Debug + PartialEq,
          S: Signal<Item = A> {

    assert_eq!(
        expected,
        // TODO a little gross
        get_all_polls(signal, (), |_, _| {})
    );
}


#[allow(dead_code)]
#[must_use = "Source does nothing unless polled"]
pub struct Source<A> {
    changes: Vec<Poll<A>>,
}

impl<A> Unpin for Source<A> {}

impl<A> Source<A> {
    #[allow(dead_code)]
    #[inline]
    pub fn new(changes: Vec<Poll<A>>) -> Self {
        Self { changes }
    }

    fn poll(&mut self, waker: &LocalWaker) -> Poll<Option<A>> {
        if self.changes.len() > 0 {
            match self.changes.remove(0) {
                Poll::Pending => {
                    waker.wake();
                    Poll::Pending
                },
                Poll::Ready(change) => Poll::Ready(Some(change)),
            }

        } else {
            Poll::Ready(None)
        }
    }
}

impl<A> Signal for Source<A> {
    type Item = A;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        self.poll(waker)
    }
}

impl<A> SignalVec for Source<VecDiff<A>> {
    type Item = A;

    #[inline]
    fn poll_vec_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        self.poll(waker)
    }
}
