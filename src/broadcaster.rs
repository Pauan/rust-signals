use std::sync::{Arc, Mutex, Weak};
use std::sync::atomic::{AtomicBool, Ordering};

use futures_core::Async;
use futures_core::task::{Context, Waker, Wake};

use super::signal::*;

/// Wraps any Signal to make it Clone.
pub struct Broadcaster<A: Signal>
    where <A as Signal>::Item: Clone
{
    state: Arc<BroadcasterState<A>>,
}

impl<A: Signal> Broadcaster<A>
    where <A as Signal>::Item: Clone  {
    pub fn new(signal: A) -> Self {

        let status = Arc::new(Mutex::new(BroadcasterStatus {
            has_changed: false,
            waker: None
        }));

        let notifier = Arc::new(BroadcasterNotifier {
            is_waiting: AtomicBool::new(false),
            targets: Mutex::new(vec![Arc::downgrade(&status)])
        });

        let shared_state = Arc::new(Mutex::new(BroadcasterSharedState {
            signal: signal,
            value: None,
            notifier: notifier
        }));

        let state = Arc::new(BroadcasterState {
            status: status,
            shared_state: shared_state
        });

        Self {
            state: state
        }
    }
}

impl<A: Signal> Signal for Broadcaster<A>
    where <A as Signal>::Item: Clone  {
    type Item = <A as Signal>::Item;

    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        // NB we musn't hold the lock on self.state.status during the call to
        // shared_state.poll_change(), as that must access all the children to notify them.

        // Don't need any potential waker as we're already polling
        self.state.status.lock().unwrap().waker = None;

        let mut shared_state = self.state.shared_state.lock().unwrap();
        if let Async::Ready(value) = shared_state.poll_change(cx) {
            // value was just broadcast
            self.state.status.lock().unwrap().has_changed = false;
            Async::Ready(value)
        } else {
            let mut status = self.state.status.lock().unwrap();
            if status.has_changed {
                // a new value was polled and broadcast by another task since we last polled
                status.has_changed = false;
                Async::Ready(shared_state.value.clone())
            } else {
                // nothing new to report
                status.waker = Some(cx.waker().clone());
                Async::Pending
            }
        }
    }
}

impl<A: Signal> Clone for Broadcaster<A>
    where <A as Signal>::Item: Clone {
    fn clone(&self) -> Self {

        let new_status = Arc::new(Mutex::new(BroadcasterStatus {
            has_changed: self.state.status.lock().unwrap().has_changed,
            waker: None
        }));

        let shared_state = self.state.shared_state.lock().unwrap();
        shared_state.notifier.targets.lock().unwrap().push(Arc::downgrade(&new_status));

        let new_state = Arc::new(BroadcasterState {
            status: new_status,
            shared_state: self.state.shared_state.clone()
        });

        Self {
            state: new_state,
        }
    }
}

// ----------------------------------------------------------------------------------------------

// State for a single broadcaster instance.
//
// It's split in this way because the BroadcasterSharedState also needs to access the status
// (e.g. to notify of changes).
struct BroadcasterState<A: Signal> {
    status: Arc<Mutex<BroadcasterStatus>>,
    shared_state: Arc<Mutex<BroadcasterSharedState<A>>>,
}

// ---------------------------------------------------------------------------------------------

struct BroadcasterStatus {
    has_changed: bool,
    waker: Option<Waker>
}

// ---------------------------------------------------------------------------------------------

// Shared state underpinning a Cloned set of broadcasters
struct BroadcasterSharedState<A: Signal> {
    signal: A,
    value: Option<A::Item>,
    notifier: Arc<BroadcasterNotifier>
}

impl<A: Signal> BroadcasterSharedState<A>
        where <A as Signal>::Item: Clone {

    // Poll the underlying signal for changes, giving it a BroadcasterWaker to wake in
    // the future if it is in Pending state.
    //
    // Takes self by Arc<Mutex<Self>> because this construct is needed to build a
    // BroadcasterWaker
    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<A::Item>> {

        // Set the notifier to be waiting on a poll, and if it wasn't previously, actually
        // execute that poll.
        //
        // TODO: Do we actually need AcqRel guarantee?
        if !self.notifier.is_waiting.swap(true, Ordering::AcqRel) {
            let waker = Waker::from(self.notifier.clone());
            let mut sub_cx = cx.with_waker(&waker);

            if let Async::Ready(value) = self.signal.poll_change(&mut sub_cx) {
                self.value = value.clone();
                self.notifier.notify();
                self.notifier.is_waiting.store(false, Ordering::Release);
                return Async::Ready(value)
            }
        }

        Async::Pending
    }
}

// ----------------------------------------------------------------------------------------------

/// This is responsible for propagating a "wake" down to any pending tasks attached to
/// broadcasted children.
struct BroadcasterNotifier {
    is_waiting: AtomicBool,
    targets: Mutex<Vec<Weak<Mutex<BroadcasterStatus>>>>
}

impl BroadcasterNotifier {
    fn notify(&self) {
        // Take this opportunity to GC dead children
        self.targets.lock().unwrap().retain(|weak_child_state| {
            if let Some(child_status_mutex) = weak_child_state.upgrade() {
                let mut child_status = child_status_mutex.lock().unwrap();
                child_status.has_changed = true;

                if let Some(waker) = child_status.waker.take() {
                    waker.wake();
                }
                true
            } else {
                false
            }
        });
    }
}

impl Wake for BroadcasterNotifier
{
    fn wake(arc_self: &Arc<Self>) {
        arc_self.notify();
        arc_self.is_waiting.store(false, Ordering::Release);
    }
}
