use super::Signal;
use std::pin::Pin;
use std::marker::Unpin;
use std::fmt::Debug;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Poll, Waker, Context};
use futures_util::task::{self, ArcWake};
use crate::signal::ChangedWaker;


/// When the Signal changes it will wake up the BroadcasterNotifier, which will
/// then wake up all of the child ChangedWaker.
#[derive(Debug)]
struct BroadcasterNotifier {
    is_changed: AtomicBool,
    wakers: Mutex<Vec<Weak<ChangedWaker>>>,
}

impl BroadcasterNotifier {
    fn new() -> Self {
        Self {
            is_changed: AtomicBool::new(true),
            wakers: Mutex::new(vec![]),
        }
    }

    fn notify(&self) {
        let mut lock = self.wakers.lock().unwrap();

        self.is_changed.store(true, Ordering::SeqCst);

        // Take this opportunity to GC dead wakers
        lock.retain(|waker| {
            if let Some(waker) = waker.upgrade() {
                waker.wake(false);
                true

            } else {
                false
            }
        });
    }

    fn is_changed(&self) -> bool {
        self.is_changed.swap(false, Ordering::SeqCst)
    }
}

impl ArcWake for BroadcasterNotifier {
    #[inline]
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.notify();
    }
}


/// This will poll the input Signal and keep track of the most recent value.
#[derive(Debug)]
struct BroadcasterInnerState<A> where A: Signal {
    // TODO is there a more efficient way to implement this ?
    signal: Option<Pin<Box<A>>>,
    waker: Waker,
    value: Option<A::Item>,
    // This is used to keep track of when the value changes
    epoch: usize,
}

impl<A> BroadcasterInnerState<A> where A: Signal {
    fn new(signal: A, waker: Waker) -> Self {
        Self {
            signal: Some(Box::pin(signal)),
            waker,
            value: None,
            epoch: 0,
        }
    }

    fn poll_signal(&mut self) {
        if let Some(ref mut signal) = self.signal {
            let cx = &mut Context::from_waker(&self.waker);

            let mut changed = false;

            loop {
                // TODO what if it is woken up while polling ?
                match signal.as_mut().poll_change(cx) {
                    Poll::Ready(Some(value)) => {
                        self.value = Some(value);
                        changed = true;
                        continue;
                    },
                    Poll::Ready(None) => {
                        self.signal = None;
                        break;
                    },
                    Poll::Pending => {
                        break;
                    },
                }
            }

            if changed {
                self.epoch += 1;
            }
        }
    }
}


/// Shared state for the Broadcaster and all child signals.
struct BroadcasterSharedState<A> where A: Signal {
    inner: RwLock<BroadcasterInnerState<A>>,
    notifier: Arc<BroadcasterNotifier>,
}

impl<A> BroadcasterSharedState<A> where A: Signal {
    fn new(signal: A) -> Self {
        let notifier = Arc::new(BroadcasterNotifier::new());
        let waker = task::waker(notifier.clone());

        Self {
            inner: RwLock::new(BroadcasterInnerState::new(signal, waker)),
            notifier,
        }
    }

    fn poll<B, F>(&self, f: F) -> B where F: FnOnce(&BroadcasterInnerState<A>) -> B {
        if self.notifier.is_changed() {
            let mut lock = self.inner.write().unwrap();

            lock.poll_signal();

            f(&lock)

        } else {
            let lock = self.inner.read().unwrap();

            f(&lock)
        }
    }
}

// TODO use derive
impl<A> Debug for BroadcasterSharedState<A>
    where A: Debug + Signal,
          A::Item: Debug {

    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        fmt.debug_struct("BroadcasterSharedState")
            .field("inner", &self.inner)
            .field("notifier", &self.notifier)
            .finish()
    }
}


struct BroadcasterState<A> where A: Signal {
    epoch: usize,
    waker: Arc<ChangedWaker>,
    shared_state: Arc<BroadcasterSharedState<A>>,
}

impl<A> BroadcasterState<A> where A: Signal {
    fn new(shared_state: &Arc<BroadcasterSharedState<A>>) -> Self {
        let waker = Arc::new(ChangedWaker::new());

        {
            let mut lock = shared_state.notifier.wakers.lock().unwrap();
            lock.push(Arc::downgrade(&waker));
        }

        Self {
            epoch: 0,
            waker,
            shared_state: shared_state.clone(),
        }
    }

    fn poll_change<B, F>(&mut self, cx: &mut Context, f: F) -> Poll<Option<B>> where F: FnOnce(&A::Item) -> B {
        let BroadcasterState { epoch, waker, shared_state } = self;

        shared_state.poll(|state| {
            // Value hasn't changed
            if state.epoch == *epoch {
                if state.signal.is_none() {
                    Poll::Ready(None)

                } else {
                    waker.set_waker(cx);
                    Poll::Pending
                }

            } else {
                *epoch = state.epoch;
                Poll::Ready(Some(f(state.value.as_ref().unwrap())))
            }
        })
    }
}

// TODO use derive
impl<A> Debug for BroadcasterState<A>
    where A: Debug + Signal,
          A::Item: Debug {

    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        fmt.debug_struct("BroadcasterState")
            .field("epoch", &self.epoch)
            .field("waker", &self.waker)
            .field("shared_state", &self.shared_state)
            .finish()
    }
}

// ---------------------------------------------------------------------------

/// Splits an input `Signal` into multiple output `Signals`.
///
/// `Broadcaster` provides `.signal()`, `.signal_cloned()`, and `.signal_ref()`
/// methods which can be used to create multiple signals from a single signal.
///
/// This is useful because `Signal` usually does not implement `Clone`, so it is
/// necessary to use `Broadcaster` to "clone" a `Signal`.
///
/// If you are using a `Mutable` then you don't need `Broadcaster`, because
/// `Mutable` already supports the `.signal()`, `.signal_cloned()` and
/// `.signal_ref()` methods (they are faster than `Broadcaster`).
pub struct Broadcaster<A> where A: Signal {
    shared_state: Arc<BroadcasterSharedState<A>>,
}

impl<A> Broadcaster<A> where A: Signal {
    /// Create a new `Broadcaster`
    pub fn new(signal: A) -> Self {
        Self {
            shared_state: Arc::new(BroadcasterSharedState::new(signal)),
        }
    }

    #[inline]
    pub fn signal_ref<B, F>(&self, f: F) -> BroadcasterSignalRef<A, F>
        where F: FnMut(&A::Item) -> B {
        BroadcasterSignalRef {
            state: BroadcasterState::new(&self.shared_state),
            callback: f,
        }
    }
}

impl<A> Broadcaster<A> where A: Signal, A::Item: Copy {
    /// Returns a new `Signal` which copies values from the input `Signal`
    #[inline]
    pub fn signal(&self) -> BroadcasterSignal<A> {
        BroadcasterSignal {
            state: BroadcasterState::new(&self.shared_state),
        }
    }
}

impl<A> Broadcaster<A> where A: Signal, A::Item: Clone {
    /// Returns a new `Signal` which clones values from the input `Signal`
    #[inline]
    pub fn signal_cloned(&self) -> BroadcasterSignalCloned<A> {
        BroadcasterSignalCloned {
            state: BroadcasterState::new(&self.shared_state),
        }
    }
}

// TODO use derive
impl<A> Debug for Broadcaster<A>
    where A: Debug + Signal,
          A::Item: Debug {

    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        fmt.debug_struct("Broadcaster")
            .field("shared_state", &self.shared_state)
            .finish()
    }
}

// ---------------------------------------------------------------------------

#[must_use = "Signals do nothing unless polled"]
pub struct BroadcasterSignal<A> where A: Signal {
    state: BroadcasterState<A>,
}

impl<A> Signal for BroadcasterSignal<A>
    where A: Signal,
          A::Item: Copy {

    type Item = A::Item;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.state.poll_change(cx, |value| *value)
    }
}

// TODO use derive
impl<A> Debug for BroadcasterSignal<A>
    where A: Debug + Signal,
          A::Item: Debug {

    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        fmt.debug_struct("BroadcasterSignal")
            .field("state", &self.state)
            .finish()
    }
}

// --------------------------------------------------------------------------

#[must_use = "Signals do nothing unless polled"]
pub struct BroadcasterSignalCloned<A> where A: Signal {
    state: BroadcasterState<A>,
}

impl<A> Signal for BroadcasterSignalCloned<A>
    where A: Signal,
          A::Item: Clone {

    type Item = A::Item;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.state.poll_change(cx, |value| value.clone())
    }
}

// TODO use derive
impl<A> Debug for BroadcasterSignalCloned<A>
    where A: Debug + Signal,
          A::Item: Debug {

    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        fmt.debug_struct("BroadcasterSignalCloned")
            .field("state", &self.state)
            .finish()
    }
}

// --------------------------------------------------------------------------

#[must_use = "Signals do nothing unless polled"]
pub struct BroadcasterSignalRef<A, F> where A: Signal {
    state: BroadcasterState<A>,
    callback: F,
}

impl<A, F> Unpin for BroadcasterSignalRef<A, F> where A: Signal {}

impl<A, B, F> Signal for BroadcasterSignalRef<A, F>
    where A: Signal,
          F: FnMut(&A::Item) -> B {

    type Item = B;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let BroadcasterSignalRef { state, callback } = &mut *self;
        state.poll_change(cx, callback)
    }
}

// TODO use derive
impl<A, F> Debug for BroadcasterSignalRef<A, F>
    where A: Debug + Signal,
          A::Item: Debug {

    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        fmt.debug_struct("BroadcasterSignalRef")
            .field("state", &self.state)
            .finish()
    }
}
