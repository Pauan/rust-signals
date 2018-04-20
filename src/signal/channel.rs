use super::Signal;
// TODO use parking_lot ?
use std::sync::{Arc, Weak, Mutex, MutexGuard};
// TODO use parking_lot ?
use futures_core::Async;
use futures_core::task::{Context, Waker};


struct Inner<A> {
    value: Option<A>,
    waker: Option<Waker>,
    dropped: bool,
}

impl<A> Inner<A> {
    fn notify(mut lock: MutexGuard<Self>) {
        if let Some(waker) = lock.waker.take() {
            drop(lock);
            waker.wake();
        }
    }
}

pub struct Sender<A> {
    inner: Weak<Mutex<Inner<A>>>,
}

impl<A> Sender<A> {
    pub fn send(&self, value: A) -> Result<(), A> {
        if let Some(inner) = self.inner.upgrade() {
            let mut inner = inner.lock().unwrap();

            inner.value = Some(value);

            Inner::notify(inner);

            Ok(())

        } else {
            Err(value)
        }
    }
}

impl<A> Drop for Sender<A> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            let mut inner = inner.lock().unwrap();

            inner.dropped = true;

            Inner::notify(inner);
        }
    }
}


pub struct Receiver<A> {
    inner: Arc<Mutex<Inner<A>>>,
}

impl<A> Signal for Receiver<A> {
    type Item = A;

    #[inline]
    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        let mut inner = self.inner.lock().unwrap();

        // TODO is this correct ?
        match inner.value.take() {
            None => if inner.dropped {
                Async::Ready(None)

            } else {
                inner.waker = Some(cx.waker().clone());
                Async::Pending
            },

            a => Async::Ready(a),
        }
    }
}

pub fn channel<A>(initial_value: A) -> (Sender<A>, Receiver<A>) {
    let inner = Arc::new(Mutex::new(Inner {
        value: Some(initial_value),
        waker: None,
        dropped: false,
    }));

    let sender = Sender {
        inner: Arc::downgrade(&inner),
    };

    let receiver = Receiver {
        inner,
    };

    (sender, receiver)
}
