use super::Signal;
use std::pin::Pin;
use std::marker::Unpin;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Poll, Waker, Context};
use futures_util::task::ArcWake;


#[derive(Debug)]
struct BroadcasterStatus {
    is_changed: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl BroadcasterStatus {
    fn new() -> Self {
        Self {
            is_changed: AtomicBool::new(true),
            waker: Mutex::new(None),
        }
    }
}

// ---------------------------------------------------------------------------

/// This is responsible for propagating a "wake" down to any pending tasks
/// attached to broadcasted children.
#[derive(Debug)]
struct BroadcasterNotifier {
    is_changed: AtomicBool,
    targets: Mutex<Vec<Weak<BroadcasterStatus>>>,
}

impl BroadcasterNotifier {
    fn new() -> Self {
        Self {
            is_changed: AtomicBool::new(true),
            targets: Mutex::new(vec![]),
        }
    }

    fn notify(&self, is_changed: bool) {
        let mut lock = self.targets.lock().unwrap();

        if is_changed {
            self.is_changed.store(true, Ordering::SeqCst);
        }

        // Take this opportunity to GC dead children
        lock.retain(|weak_child_state| {
            if let Some(child_status) = weak_child_state.upgrade() {
                let mut lock = child_status.waker.lock().unwrap();

                if is_changed {
                    child_status.is_changed.store(true, Ordering::SeqCst);
                }

                if let Some(waker) = lock.take() {
                    drop(lock);
                    waker.wake();
                }

                true

            } else {
                false
            }
        });
    }
}

impl ArcWake for BroadcasterNotifier {
    #[inline]
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.notify(true);
    }
}

// ---------------------------------------------------------------------------

#[derive(Debug)]
struct BroadcasterInnerState<A> where A: Signal {
    // TODO is there a more efficient way to implement this ?
    signal: Pin<Box<A>>,
    value: Option<A::Item>,
}

impl<A> BroadcasterInnerState<A> where A: Signal {
    fn new(signal: A) -> Self {
        Self {
            signal: Box::pin(signal),
            value: None,
        }
    }

    // Poll the underlying signal for changes, giving it a BroadcasterNotifier
    // to wake in the future if it is in Pending state.
    fn poll_underlying(&mut self, notifier: Arc<BroadcasterNotifier>) {
        // TODO is this the best way to do this ?
        let waker = ArcWake::into_waker(notifier);
        let cx = &mut Context::from_waker(&waker);

        loop {
            // TODO what if it is woken up while polling ?
            // TODO is this safe for pinned types ?
            match self.signal.as_mut().poll_change(cx) {
                Poll::Ready(value) => {
                    let done = value.is_none();

                    self.value = value;

                    if done {
                        break;
                    }
                },
                Poll::Pending => {
                    break;
                },
            }
        }
    }
}

// TODO is this correct ?
impl<A> Unpin for BroadcasterInnerState<A> where A: Signal {}

// ---------------------------------------------------------------------------

// Shared state underpinning a Cloned set of broadcasters
struct BroadcasterSharedState<A> where A: Signal {
    inner: RwLock<BroadcasterInnerState<A>>,
    notifier: Arc<BroadcasterNotifier>,
}

impl<A> BroadcasterSharedState<A> where A: Signal {
    fn new(signal: A) -> Self {
        Self {
            inner: RwLock::new(BroadcasterInnerState::new(signal)),
            notifier: Arc::new(BroadcasterNotifier::new()),
        }
    }

    fn poll<B, F>(&self, f: F) -> B where F: FnOnce(&Option<A::Item>) -> B {
        // TODO is this correct ?
        if self.notifier.is_changed.swap(false, Ordering::SeqCst) {
            let mut lock = self.inner.write().unwrap();

            lock.poll_underlying(self.notifier.clone());

            f(&lock.value)

        } else {
            let lock = self.inner.read().unwrap();

            f(&lock.value)
        }
    }
}

// TODO use derive
impl<A> ::std::fmt::Debug for BroadcasterSharedState<A>
    where A: ::std::fmt::Debug + Signal,
          A::Item: ::std::fmt::Debug {

    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        fmt.debug_struct("BroadcasterSharedState")
            .field("inner", &self.inner)
            .field("notifier", &self.notifier)
            .finish()
    }
}

// ---------------------------------------------------------------------------

// State for a single broadcaster instance.
//
// It's split in this way because the BroadcasterSharedState also needs to
// access the status (e.g. to notify of changes).
struct BroadcasterState<A> where A: Signal {
    status: Arc<BroadcasterStatus>,
    shared_state: Arc<BroadcasterSharedState<A>>,
}

impl<A> BroadcasterState<A> where A: Signal {
    fn new(shared_state: &Arc<BroadcasterSharedState<A>>) -> Self {
        let new_status = Arc::new(BroadcasterStatus::new());

        {
            let mut lock = shared_state.notifier.targets.lock().unwrap();
            lock.push(Arc::downgrade(&new_status));
        }

        Self {
            status: new_status,
            shared_state: shared_state.clone(),
        }
    }

    fn poll_change<F>(&self, cx: &mut Context, f: F) -> Poll<Option<A::Item>> where F: FnOnce(&Option<A::Item>) -> Option<A::Item> {
        // If the poll just done (or a previous poll) has generated a new
        // value, we can report it. Use swap so only one thread will pick up
        // the change
        if self.status.is_changed.swap(false, Ordering::SeqCst) {
            Poll::Ready(self.shared_state.poll(f))

        } else {
            // Nothing new to report, save this task's Waker for later
            *self.status.waker.lock().unwrap() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

// TODO use derive
impl<A> ::std::fmt::Debug for BroadcasterState<A>
    where A: ::std::fmt::Debug + Signal,
          A::Item: ::std::fmt::Debug {

    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        fmt.debug_struct("BroadcasterState")
            .field("status", &self.status)
            .field("shared_state", &self.shared_state)
            .finish()
    }
}

// ---------------------------------------------------------------------------

/// Wraps any `Signal` to make it possible to "broadcast" it to several
/// consumers.
///
/// `Broadcaster` provides the `.signal()` and `.signal_cloned()` methods which
/// can be used to produce multiple signals out of the one original signal
/// the `Broadcaster` was created with.
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
}

impl<A> Broadcaster<A> where A: Signal, A::Item: Copy {
    /// Create a new `Signal` which copies values from the `Signal` wrapped
    /// by the `Broadcaster`
    // TODO: use `impl Signal` for the return type
    pub fn signal(&self) -> BroadcasterSignal<A> {
        BroadcasterSignal {
            state: BroadcasterState::new(&self.shared_state),
        }
    }
}

impl<A> Broadcaster<A> where A: Signal, A::Item: Clone {
    /// Create a new `Signal` which clones values from the `Signal` wrapped
    /// by the `Broadcaster`
    // TODO: use `impl Signal` for the return type
    pub fn signal_cloned(&self) -> BroadcasterSignalCloned<A> {
        BroadcasterSignalCloned {
            state: BroadcasterState::new(&self.shared_state),
        }
    }
}

// TODO use derive
impl<A> ::std::fmt::Debug for Broadcaster<A>
    where A: ::std::fmt::Debug + Signal,
          A::Item: ::std::fmt::Debug {

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
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.state.poll_change(cx, |value| *value)
    }
}

// TODO use derive
impl<A> ::std::fmt::Debug for BroadcasterSignal<A>
    where A: ::std::fmt::Debug + Signal,
          A::Item: ::std::fmt::Debug {

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
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.state.poll_change(cx, |value| value.clone())
    }
}

// TODO use derive
impl<A> ::std::fmt::Debug for BroadcasterSignalCloned<A>
    where A: ::std::fmt::Debug + Signal,
          A::Item: ::std::fmt::Debug {

    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        fmt.debug_struct("BroadcasterSignalCloned")
            .field("state", &self.state)
            .finish()
    }
}
