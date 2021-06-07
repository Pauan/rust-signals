use super::Signal;
use std::pin::Pin;
use std::marker::Unpin;
use std::sync::{Arc, Weak};
use parking_lot::{Mutex, MutexGuard};
use std::task::{Poll, Context, Waker};


#[derive(Debug)]
struct Inner<A> {
    value: Option<A>,
    waker: Option<Waker>,
    senders: usize,
}

impl<A> Inner<A> {
    fn notify(mut lock: MutexGuard<Self>) {
        if let Some(waker) = lock.waker.take() {
            drop(lock);
            waker.wake();
        }
    }
}


#[derive(Debug)]
pub struct Sender<A> {
    inner: Weak<Mutex<Inner<A>>>,
}

impl<A> Sender<A> {
    pub fn send(&self, value: A) -> Result<(), A> {
        if let Some(inner) = self.inner.upgrade() {
            let mut inner = inner.lock();

            // This will be 0 if the channel was closed
            if inner.senders > 0 {
                inner.value = Some(value);

                Inner::notify(inner);

                Ok(())

            } else {
                Err(value)
            }

        } else {
            Err(value)
        }
    }

    pub fn close(&self) {
        if let Some(inner) = self.inner.upgrade() {
            let mut inner = inner.lock();

            // This will be 0 if the channel was closed
            if inner.senders > 0 {
                inner.senders = 0;

                Inner::notify(inner);
            }
        }
    }
}

impl<A> Clone for Sender<A> {
    fn clone(&self) -> Self {
        if let Some(inner) = self.inner.upgrade() {
            let mut inner = inner.lock();

            // This will be 0 if the channel was closed
            if inner.senders > 0 {
                inner.senders += 1;
            }
        }

        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<A> Drop for Sender<A> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            let mut inner = inner.lock();

            // This will be 0 if the channel was closed
            if inner.senders > 0 {
                inner.senders -= 1;

                if inner.senders == 0 {
                    Inner::notify(inner);
                }
            }
        }
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Receiver<A> {
    inner: Arc<Mutex<Inner<A>>>,
}

impl<A> Unpin for Receiver<A> {}

impl<A> Signal for Receiver<A> {
    type Item = A;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut inner = self.inner.lock();

        // TODO is this correct ?
        match inner.value.take() {
            None => if inner.senders == 0 {
                Poll::Ready(None)

            } else {
                inner.waker = Some(cx.waker().clone());
                Poll::Pending
            },

            a => Poll::Ready(a),
        }
    }
}

pub fn channel<A>(initial_value: A) -> (Sender<A>, Receiver<A>) {
    let inner = Arc::new(Mutex::new(Inner {
        value: Some(initial_value),
        waker: None,
        senders: 1,
    }));

    let sender = Sender {
        inner: Arc::downgrade(&inner),
    };

    let receiver = Receiver {
        inner,
    };

    (sender, receiver)
}
