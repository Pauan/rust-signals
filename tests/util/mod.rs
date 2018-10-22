use futures_signals::signal::Signal;
use futures_core::Poll;
use futures_core::task::{LocalWaker, Wake, local_waker_from_nonlocal};
use futures_util::future::poll_fn;
use futures_executor::block_on;
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
pub fn get_all_polls<A, B, F>(mut signal: A, mut initial: B, mut f: F) -> Vec<Poll<Option<A::Item>>> where A: Signal, F: FnMut(&B, &LocalWaker) -> B {
    let mut output = vec![];

    block_on(poll_fn(|waker| {
        loop {
            initial = f(&initial, waker);

            // TODO is this safe ?
            let x = unsafe { Pin::new_unchecked(&mut signal) }.poll_change(waker);

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
