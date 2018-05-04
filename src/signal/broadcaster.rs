use super::Signal;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use futures_core::Async;
use futures_core::task::{Context, Waker, Wake};


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

impl Wake for BroadcasterNotifier {
    #[inline]
    fn wake(arc_self: &Arc<Self>) {
        arc_self.notify(true);
    }
}

// ---------------------------------------------------------------------------

struct BroadcasterInnerState<A> where A: Signal {
    signal: A,
    value: Option<A::Item>,
}

impl<A> BroadcasterInnerState<A> where A: Signal {
    fn new(signal: A) -> Self {
        Self {
            signal,
            value: None,
        }
    }

    // Poll the underlying signal for changes, giving it a BroadcasterNotifier
    // to wake in the future if it is in Pending state.
    fn poll_underlying(&mut self, cx: &mut Context, notifier: Arc<BroadcasterNotifier>) {
        let waker = Waker::from(notifier);

        let mut sub_cx = cx.with_waker(&waker);

        loop {
            // TODO what if it is woken up while polling ?
            match self.signal.poll_change(&mut sub_cx) {
                Async::Ready(value) => {
                    let done = value.is_none();

                    self.value = value;

                    if done {
                        break;
                    }
                },
                Async::Pending => {
                    break;
                },
            }
        }
    }
}

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

    fn poll<B, F>(&self, cx: &mut Context, f: F) -> B where F: FnOnce(&Option<A::Item>) -> B {
        // TODO is this correct ?
        if self.notifier.is_changed.swap(false, Ordering::SeqCst) {
            let mut lock = self.inner.write().unwrap();

            lock.poll_underlying(cx, self.notifier.clone());

            f(&lock.value)

        } else {
            let lock = self.inner.read().unwrap();

            f(&lock.value)
        }
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

    fn poll_change<F>(&self, cx: &mut Context, f: F) -> Async<Option<A::Item>> where F: FnOnce(&Option<A::Item>) -> Option<A::Item> {
        // If the poll just done (or a previous poll) has generated a new
        // value, we can report it. Use swap so only one thread will pick up
        // the change
        if self.status.is_changed.swap(false, Ordering::SeqCst) {
            Async::Ready(self.shared_state.poll(cx, f))

        } else {
            // Nothing new to report, save this task's Waker for later
            *self.status.waker.lock().unwrap() = Some(cx.waker().clone());
            Async::Pending
        }
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

// ---------------------------------------------------------------------------

pub struct BroadcasterSignal<A> where A: Signal {
    state: BroadcasterState<A>,
}

impl<A> Signal for BroadcasterSignal<A>
    where A: Signal,
          A::Item: Copy {

    type Item = A::Item;

    #[inline]
    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        self.state.poll_change(cx, |value| *value)
    }
}

// --------------------------------------------------------------------------

pub struct BroadcasterSignalCloned<A> where A: Signal {
    state: BroadcasterState<A>,
}

impl<A> Signal for BroadcasterSignalCloned<A>
    where A: Signal,
          A::Item: Clone {

    type Item = A::Item;

    #[inline]
    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        self.state.poll_change(cx, |value| value.clone())
    }
}
