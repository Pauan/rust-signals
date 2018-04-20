use std::sync::{Arc, Mutex, RwLock, Weak};
use std::sync::atomic::{AtomicBool, Ordering};

use futures_core::Async;
use futures_core::task::{Context, Waker, Wake};

use super::signal::*;

/// Wraps any `Signal` to make it possible to "broadcast" it to several
/// consumers.
///
/// `Broadcaster` provides the `.signal()` and `.signal_cloned()` methods which
/// can be used to produce multiple signals out of the one original signal
/// the `Broadcaster` was created with.
pub struct Broadcaster<A: Signal> where <A as Signal>::Item: Clone {
    shared_state: Arc<BroadcasterSharedState<A>>,
}

impl<A: Signal> Broadcaster<A> where <A as Signal>::Item: Clone  {

    /// Create a new `Broadcaster`
    pub fn new(signal: A) -> Self {
        let notifier = Arc::new(BroadcasterNotifier {
            is_waiting: AtomicBool::new(false),
            targets: Mutex::new(vec![])
        });

        let shared_state = Arc::new(BroadcasterSharedState {
            signal: Mutex::new(signal),
            value: RwLock::new(None),
            notifier: notifier
        });

        Self {
            shared_state: shared_state
        }
    }

    /// Create a new `Signal` which clones values from the `Signal` wrapped
    /// by the `Broadcaster`
    ///
    /// TODO: use `impl Signal` for the return type?
    pub fn signal_cloned(&self) -> BroadcasterSignalCloned<A> {
        let new_status = Arc::new(BroadcasterStatus {
            has_changed: AtomicBool::new(true),
            waker: Mutex::new(None)
        });

        self.shared_state.notifier.targets.lock().unwrap()
            .push(Arc::downgrade(&new_status));

        let new_state = Box::new(BroadcasterState {
            status: new_status,
            shared_state: self.shared_state.clone()
        });

        BroadcasterSignalCloned {
            state: new_state,
        }
    }
}

impl<A: Signal> Broadcaster<A> where <A as Signal>::Item: Copy {

    /// Create a new `Signal` which copies values from the `Signal` wrapped
    /// by the `Broadcaster`
    ///
    /// TODO: use `impl Signal` for the return type?
    pub fn signal(&self) -> BroadcasterSignal<A> {
        let new_status = Arc::new(BroadcasterStatus {
            has_changed: AtomicBool::new(true),
            waker: Mutex::new(None)
        });

        self.shared_state.notifier.targets.lock().unwrap()
            .push(Arc::downgrade(&new_status));

        let new_state = Box::new(BroadcasterState {
            status: new_status,
            shared_state: self.shared_state.clone()
        });

        BroadcasterSignal {
            state: new_state,
        }
    }
}

// ---------------------------------------------------------------------------

pub struct BroadcasterSignal<A: Signal> where <A as Signal>::Item: Copy {
    state: Box<BroadcasterState<A>>
}

impl<A: Signal> Signal for BroadcasterSignal<A>
        where <A as Signal>::Item: Copy {
    type Item = <A as Signal>::Item;

    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        // Don't need any potential waker as this task is now awake!
        *self.state.status.waker.lock().unwrap() = None;

        // Check for changes in the underlying signal (if not waiting already).
        self.state.shared_state.poll_underlying(cx);

        // If the poll just done (or a previous poll) has generated a new
        // value, we can report it. Use swap so only one thread will pick up
        // the change
        let status = &*self.state.status;
        if status.has_changed.swap(false, Ordering::SeqCst) {
            Async::Ready(*self.state.shared_state.value.read().unwrap())
        } else {
            // Nothing new to report, save this task's Waker for later
            *status.waker.lock().unwrap() = Some(cx.waker().clone());
            Async::Pending
        }
    }
}

// --------------------------------------------------------------------------

pub struct BroadcasterSignalCloned<A: Signal> where <A as Signal>::Item: Clone {
    state: Box<BroadcasterState<A>>
}

impl<A: Signal> Signal for BroadcasterSignalCloned<A>
        where <A as Signal>::Item: Clone {
    type Item = <A as Signal>::Item;

    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        // Don't need any potential waker as this task is now awake!
        *self.state.status.waker.lock().unwrap() = None;

        // Check for changes in the underlying signal (if not waiting already).
        self.state.shared_state.poll_underlying(cx);

        // If the poll just done (or a previous poll) has generated a new
        // value, we can report it. Use swap so only one thread will pick up
        // the change
        let status = &*self.state.status;
        if status.has_changed.swap(false, Ordering::SeqCst) {
            Async::Ready(self.state.shared_state.value.read().unwrap().clone())
        } else {
            // Nothing new to report, save this task's Waker for later
            *status.waker.lock().unwrap() = Some(cx.waker().clone());
            Async::Pending
        }
    }
}

// ---------------------------------------------------------------------------

// State for a single broadcaster instance.
//
// It's split in this way because the BroadcasterSharedState also needs to
// access the status (e.g. to notify of changes).
struct BroadcasterState<A: Signal> {
    status: Arc<BroadcasterStatus>,
    shared_state: Arc<BroadcasterSharedState<A>>,
}

// ---------------------------------------------------------------------------

struct BroadcasterStatus {
    has_changed: AtomicBool,
    waker: Mutex<Option<Waker>>
}

// ---------------------------------------------------------------------------

// Shared state underpinning a Cloned set of broadcasters
struct BroadcasterSharedState<A: Signal> {
    signal: Mutex<A>,
    value: RwLock<Option<A::Item>>,
    notifier: Arc<BroadcasterNotifier>
}

impl<A: Signal> BroadcasterSharedState<A> where <A as Signal>::Item: Clone {

    // Poll the underlying signal for changes, giving it a BroadcasterNotifier
    // to wake in the future if it is in Pending state.
    //
    // If the underlying `Signal` generates `Async::Ready`, then all the
    // `BroadcasterSignal`s in the group will have their `has_changed` flag
    // set so they can pick up the new value with their next poll.
    fn poll_underlying(&self, cx: &mut Context) {

        // Set the notifier to be waiting on a poll, and if it wasn't
        // previously, actually execute that poll.
        if !self.notifier.is_waiting.swap(true, Ordering::SeqCst) {
            let waker = Waker::from(self.notifier.clone());
            let mut sub_cx = cx.with_waker(&waker);
            let change = self.signal.lock().unwrap().poll_change(&mut sub_cx);

            if let Async::Ready(value) = change {
                *self.value.write().unwrap() = value;
                self.notifier.notify();
                self.notifier.is_waiting.store(false, Ordering::SeqCst);
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// This is responsible for propagating a "wake" down to any pending tasks
/// attached to broadcasted children.
struct BroadcasterNotifier {
    is_waiting: AtomicBool,
    targets: Mutex<Vec<Weak<BroadcasterStatus>>>
}

impl BroadcasterNotifier {
    fn notify(&self) {
        // Take this opportunity to GC dead children
        self.targets.lock().unwrap().retain(|weak_child_state| {
            if let Some(child_status) = weak_child_state.upgrade() {
                child_status.has_changed.store(true, Ordering::SeqCst);

                if let Some(waker) = child_status.waker.lock().unwrap().take() {
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
    fn wake(arc_self: &Arc<Self>) {
        arc_self.notify();
        arc_self.is_waiting.store(false, Ordering::SeqCst);
    }
}
