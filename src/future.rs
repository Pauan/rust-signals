use std::pin::Pin;
use parking_lot::Mutex;
use std::sync::{Arc, Weak};
use std::future::Future;
use std::task::{Poll, Waker, Context};
use std::sync::atomic::{AtomicBool, Ordering};
use discard::{Discard, DiscardOnDrop};
use pin_project::pin_project;


#[derive(Debug)]
struct CancelableFutureState {
    is_cancelled: AtomicBool,
    waker: Mutex<Option<Waker>>,
}


#[derive(Debug)]
pub struct CancelableFutureHandle {
    state: Weak<CancelableFutureState>,
}

impl Discard for CancelableFutureHandle {
    fn discard(self) {
        if let Some(state) = self.state.upgrade() {
            let mut lock = state.waker.lock();

            // TODO verify that this is correct
            state.is_cancelled.store(true, Ordering::SeqCst);

            if let Some(waker) = lock.take() {
                drop(lock);
                waker.wake();
            }
        }
    }
}


#[pin_project(project = CancelableFutureProj)]
#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct CancelableFuture<A, B> {
    state: Arc<CancelableFutureState>,
    #[pin]
    future: Option<A>,
    when_cancelled: Option<B>,
}

impl<A, B> Future for CancelableFuture<A, B>
    where A: Future,
          B: FnOnce() -> A::Output {

    type Output = A::Output;

    // TODO should this inline ?
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let CancelableFutureProj { state, mut future, when_cancelled } = self.project();

        // TODO is this correct ?
        if state.is_cancelled.load(Ordering::SeqCst) {
            // This is necessary in order to prevent the future from calling `waker.wake()` later
            future.set(None);
            let callback = when_cancelled.take().unwrap();
            // TODO figure out how to call the callback immediately when discard is called, e.g. using two Arc<Mutex<>>
            Poll::Ready(callback())

        } else {
            match future.as_pin_mut().unwrap().poll(cx) {
                Poll::Pending => {
                    // TODO is this correct ?
                    *state.waker.lock() = Some(cx.waker().clone());
                    Poll::Pending
                },
                a => a,
            }
        }
    }
}


// TODO figure out a more efficient way to implement this
// TODO replace with futures_util::abortable ?
pub fn cancelable_future<A, B>(future: A, when_cancelled: B) -> (DiscardOnDrop<CancelableFutureHandle>, CancelableFuture<A, B>)
    where A: Future,
          B: FnOnce() -> A::Output {

    let state = Arc::new(CancelableFutureState {
        is_cancelled: AtomicBool::new(false),
        waker: Mutex::new(None),
    });

    let cancel_handle = DiscardOnDrop::new(CancelableFutureHandle {
        state: Arc::downgrade(&state),
    });

    let cancel_future = CancelableFuture {
        state,
        future: Some(future),
        when_cancelled: Some(when_cancelled),
    };

    (cancel_handle, cancel_future)
}
