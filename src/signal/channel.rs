use super::Signal;
use crate::atomic::AtomicOption;
use std::marker::Unpin;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::task::{Context, Poll, Waker};

#[derive(Debug)]
struct Inner<A> {
    value: AtomicOption<A>,
    waker: AtomicOption<Waker>,
    senders: AtomicUsize,
}

impl<A> Inner<A> {
    #[inline]
    fn has_senders(&self) -> bool {
        // This will be 0 if the channel was closed
        self.senders.load(Ordering::Acquire) != 0
    }

    #[inline]
    fn add_sender(&self) {
        self.senders.fetch_add(1, Ordering::AcqRel);
    }

    /// Returns `true` if it no longer has any senders
    #[inline]
    fn remove_sender(&self) -> bool {
        self.senders.fetch_sub(1, Ordering::AcqRel) == 1
    }

    #[inline]
    fn remove_all_senders(&self) {
        self.senders.store(0, Ordering::Release);
    }

    fn notify(&self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

#[derive(Debug)]
pub struct Sender<A> {
    inner: Weak<Inner<A>>,
}

impl<A> Sender<A> {
    pub fn send(&self, value: A) -> Result<(), A> {
        if let Some(inner) = self.inner.upgrade() {
            if inner.has_senders() {
                inner.value.store(Some(value));
                inner.notify();
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
            if inner.has_senders() {
                inner.remove_all_senders();
                inner.notify();
            }
        }
    }
}

impl<A> Clone for Sender<A> {
    fn clone(&self) -> Self {
        if let Some(inner) = self.inner.upgrade() {
            // This might be `false` if the `close` method was called
            if inner.has_senders() {
                inner.add_sender();
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
            // This might be `false` if the `close` method was called
            if inner.has_senders() {
                // If there aren't any more senders...
                if inner.remove_sender() {
                    inner.notify();
                }
            }
        }
    }
}

#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Receiver<A> {
    inner: Arc<Inner<A>>,
}

impl<A> Unpin for Receiver<A> {}

impl<A> Signal for Receiver<A> {
    type Item = A;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.inner.value.take() {
            None => {
                if self.inner.has_senders() {
                    self.inner.waker.store(Some(cx.waker().clone()));
                    Poll::Pending
                } else {
                    Poll::Ready(None)
                }
            }

            a => Poll::Ready(a),
        }
    }
}

pub fn channel<A>(initial_value: A) -> (Sender<A>, Receiver<A>) {
    let inner = Arc::new(Inner {
        value: AtomicOption::new(Some(initial_value)),
        waker: AtomicOption::new(None),
        senders: AtomicUsize::new(1),
    });

    let sender = Sender {
        inner: Arc::downgrade(&inner),
    };

    let receiver = Receiver { inner };

    (sender, receiver)
}
