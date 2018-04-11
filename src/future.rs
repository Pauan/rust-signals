// TODO use parking_lot ?
use std::sync::{Arc, Weak, Mutex};
// TODO use parking_lot ?
use std::sync::atomic::{AtomicBool, Ordering};
use futures_core::task::{Context, Waker};
use futures_core::{Async, Poll};
use futures_core::future::{Future, IntoFuture};
use discard::{Discard, DiscardOnDrop};


struct CancelableFutureState {
    is_cancelled: AtomicBool,
    waker: Mutex<Option<Waker>>,
}


pub struct CancelableFutureHandle {
    state: Weak<CancelableFutureState>,
}

impl Discard for CancelableFutureHandle {
    fn discard(self) {
        if let Some(state) = self.state.upgrade() {
            let mut lock = state.waker.lock().unwrap();

            // TODO verify that this is correct
            state.is_cancelled.store(true, Ordering::SeqCst);

            if let Some(waker) = lock.take() {
                drop(lock);
                waker.wake();
            }
        }
    }
}


pub struct CancelableFuture<A, B> {
    state: Arc<CancelableFutureState>,
    future: Option<A>,
    when_cancelled: Option<B>,
}

impl<A, B> Future for CancelableFuture<A, B>
    where A: Future,
          B: FnOnce(A) -> A::Item {

    type Item = A::Item;
    type Error = A::Error;

    // TODO should this inline ?
    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Poll<Self::Item, Self::Error> {
        // TODO is this correct ?
        if self.state.is_cancelled.load(Ordering::SeqCst) {
            let future = self.future.take().unwrap();
            let callback = self.when_cancelled.take().unwrap();
            // TODO figure out how to call the callback immediately when discard is called, e.g. using two Arc<Mutex<>>
            Ok(Async::Ready(callback(future)))

        } else {
            match self.future.as_mut().unwrap().poll(cx) {
                Ok(Async::Pending) => {
                    // TODO is this correct ?
                    *self.state.waker.lock().unwrap() = Some(cx.waker().clone());
                    Ok(Async::Pending)
                },
                a => a,
            }
        }
    }
}


// TODO figure out a more efficient way to implement this
// TODO this should be implemented in the futures_core crate
pub fn cancelable_future<A, B>(future: A, when_cancelled: B) -> (DiscardOnDrop<CancelableFutureHandle>, CancelableFuture<A::Future, B>)
    where A: IntoFuture,
          B: FnOnce(A::Future) -> A::Item {

    let state = Arc::new(CancelableFutureState {
        is_cancelled: AtomicBool::new(false),
        waker: Mutex::new(None),
    });

    let cancel_handle = DiscardOnDrop::new(CancelableFutureHandle {
        state: Arc::downgrade(&state),
    });

    let cancel_future = CancelableFuture {
        state,
        future: Some(future.into_future()),
        when_cancelled: Some(when_cancelled),
    };

    (cancel_handle, cancel_future)
}
