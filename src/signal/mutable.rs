use super::Signal;
use std;
use std::fmt;
use std::pin::Pin;
use std::marker::Unpin;
use std::ops::{Deref, DerefMut};
// TODO use parking_lot ?
use std::sync::{Arc, Weak, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
// TODO use parking_lot ?
use std::sync::atomic::{AtomicBool, Ordering};
use futures_core::Poll;
use futures_core::task::{Waker};
use serde::{Serialize, Deserialize, Serializer, Deserializer};


#[derive(Debug)]
struct MutableState<A> {
    value: A,
    senders: usize,
    // TODO use HashMap or BTreeMap instead ?
    receivers: Vec<Weak<MutableSignalState<A>>>,
}

impl<A> MutableState<A> {
    fn notify(&mut self, has_changed: bool) {
        self.receivers.retain(|receiver| {
            if let Some(receiver) = receiver.upgrade() {
                let mut lock = receiver.waker.lock().unwrap();

                if has_changed {
                    // TODO verify that this is correct
                    receiver.has_changed.store(true, Ordering::SeqCst);
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


#[derive(Debug)]
struct MutableSignalState<A> {
    has_changed: AtomicBool,
    waker: Mutex<Option<Waker>>,
    // TODO change this to Weak ?
    state: Arc<RwLock<MutableState<A>>>,
}

impl<A> MutableSignalState<A> {
    fn new(mutable_state: &Arc<RwLock<MutableState<A>>>) -> Arc<Self> {
        let state = Arc::new(MutableSignalState {
            has_changed: AtomicBool::new(true),
            waker: Mutex::new(None),
            state: mutable_state.clone(),
        });

        {
            let mut lock = mutable_state.write().unwrap();

            if lock.senders != 0 {
                lock.receivers.push(Arc::downgrade(&state));
            }
        }

        state
    }

    fn poll_change<B, F>(&self, waker: &Waker, f: F) -> Poll<Option<B>> where F: FnOnce(&A) -> B {
        // TODO is this correct ?
        let lock = self.state.read().unwrap();

        // TODO verify that this is correct
        if self.has_changed.swap(false, Ordering::SeqCst) {
            Poll::Ready(Some(f(&lock.value)))

        } else if lock.senders == 0 {
            Poll::Ready(None)

        } else {
            // TODO is this correct ?
            *self.waker.lock().unwrap() = Some(waker.clone());
            Poll::Pending
        }
    }
}


#[derive(Debug)]
pub struct MutableLockMut<'a, A> where A: 'a {
    mutated: bool,
    lock: RwLockWriteGuard<'a, MutableState<A>>,
}

impl<'a, A> Deref for MutableLockMut<'a, A> {
    type Target = A;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.lock.value
    }
}

impl<'a, A> DerefMut for MutableLockMut<'a, A> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mutated = true;
        &mut self.lock.value
    }
}

impl<'a, A> Drop for MutableLockMut<'a, A> {
    #[inline]
    fn drop(&mut self) {
        if self.mutated {
            self.lock.notify(true);
        }
    }
}


#[derive(Debug)]
pub struct MutableLockRef<'a, A> where A: 'a {
    lock: RwLockReadGuard<'a, MutableState<A>>,
}

impl<'a, A> Deref for MutableLockRef<'a, A> {
    type Target = A;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.lock.value
    }
}


pub struct ReadOnlyMutable<A>(Arc<RwLock<MutableState<A>>>);

impl<A> ReadOnlyMutable<A> {
    // TODO return Result ?
    #[inline]
    pub fn lock_ref(&self) -> MutableLockRef<A> {
        MutableLockRef {
            lock: self.0.read().unwrap(),
        }
    }

    #[inline]
    pub fn signal_ref<B, F>(&self, f: F) -> MutableSignalRef<A, F> where F: FnMut(&A) -> B {
        MutableSignalRef(MutableSignalState::new(&self.0), f)
    }
}

impl<A: Copy> ReadOnlyMutable<A> {
    #[inline]
    pub fn get(&self) -> A {
        self.0.read().unwrap().value
    }

    #[inline]
    pub fn signal(&self) -> MutableSignal<A> {
        MutableSignal(MutableSignalState::new(&self.0))
    }
}

impl<A: Clone> ReadOnlyMutable<A> {
    #[inline]
    pub fn get_cloned(&self) -> A {
        self.0.read().unwrap().value.clone()
    }

    #[inline]
    pub fn signal_cloned(&self) -> MutableSignalCloned<A> {
        MutableSignalCloned(MutableSignalState::new(&self.0))
    }
}

impl<A> Clone for ReadOnlyMutable<A> {
    #[inline]
    fn clone(&self) -> Self {
        ReadOnlyMutable(self.0.clone())
    }
}

impl<A> fmt::Debug for ReadOnlyMutable<A> where A: fmt::Debug {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let state = self.0.read().unwrap();

        fmt.debug_tuple("ReadOnlyMutable")
            .field(&state.value)
            .finish()
    }
}


pub struct Mutable<A>(ReadOnlyMutable<A>);

impl<A> Mutable<A> {
    // TODO should this inline ?
    pub fn new(value: A) -> Self {
        Mutable(ReadOnlyMutable(Arc::new(RwLock::new(MutableState {
            value,
            senders: 1,
            receivers: vec![],
        }))))
    }

    #[inline]
    fn state(&self) -> &Arc<RwLock<MutableState<A>>> {
        &(self.0).0
    }

    #[inline]
    pub fn read_only(&self) -> ReadOnlyMutable<A> {
        self.0.clone()
    }

    pub fn replace(&self, value: A) -> A {
        let mut state = self.state().write().unwrap();

        let value = std::mem::replace(&mut state.value, value);

        state.notify(true);

        value
    }

    pub fn replace_with<F>(&self, f: F) -> A where F: FnOnce(&mut A) -> A {
        let mut state = self.state().write().unwrap();

        let new_value = f(&mut state.value);
        let value = std::mem::replace(&mut state.value, new_value);

        state.notify(true);

        value
    }

    pub fn swap(&self, other: &Mutable<A>) {
        // TODO can this dead lock ?
        let mut state1 = self.state().write().unwrap();
        let mut state2 = other.state().write().unwrap();

        std::mem::swap(&mut state1.value, &mut state2.value);

        state1.notify(true);
        state2.notify(true);
    }

    pub fn set(&self, value: A) {
        let mut state = self.state().write().unwrap();

        state.value = value;

        state.notify(true);
    }

    pub fn set_if<F>(&self, value: A, f: F) where F: FnOnce(&A, &A) -> bool {
        let mut state = self.state().write().unwrap();

        if f(&state.value, &value) {
            state.value = value;
            state.notify(true);
        }
    }

    // TODO lots of unit tests to verify that it only notifies when the object is mutated
    // TODO return Result ?
    // TODO should this inline ?
    pub fn lock_mut(&self) -> MutableLockMut<A> {
        MutableLockMut {
            mutated: false,
            lock: self.state().write().unwrap(),
        }
    }
}

impl<A> ::std::ops::Deref for Mutable<A> {
    type Target = ReadOnlyMutable<A>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<A: PartialEq> Mutable<A> {
    #[inline]
    pub fn set_neq(&self, value: A) {
        self.set_if(value, PartialEq::ne);
    }
}

impl<A> fmt::Debug for Mutable<A> where A: fmt::Debug {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let state = self.state().read().unwrap();

        fmt.debug_tuple("Mutable")
            .field(&state.value)
            .finish()
    }
}

impl<T> Serialize for Mutable<T> where T: Serialize {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
        self.state().read().unwrap().value.serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for Mutable<T> where T: Deserialize<'de> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de> {
        T::deserialize(deserializer).map(Mutable::new)
    }
}

// TODO can this be derived ?
impl<T: Default> Default for Mutable<T> {
    #[inline]
    fn default() -> Self {
        Mutable::new(Default::default())
    }
}

impl<A> Clone for Mutable<A> {
    #[inline]
    fn clone(&self) -> Self {
        self.state().write().unwrap().senders += 1;
        Mutable(self.0.clone())
    }
}

impl<A> Drop for Mutable<A> {
    #[inline]
    fn drop(&mut self) {
        let mut state = self.state().write().unwrap();

        state.senders -= 1;

        if state.senders == 0 && state.receivers.len() > 0 {
            state.notify(false);
            // TODO is this necessary ?
            state.receivers = vec![];
        }
    }
}


// TODO remove it from receivers when it's dropped
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct MutableSignal<A>(Arc<MutableSignalState<A>>);

impl<A> Unpin for MutableSignal<A> {}

impl<A: Copy> Signal for MutableSignal<A> {
    type Item = A;

    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        self.0.poll_change(waker, |value| *value)
    }
}


// TODO remove it from receivers when it's dropped
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct MutableSignalRef<A, F>(Arc<MutableSignalState<A>>, F);

impl<A, F> Unpin for MutableSignalRef<A, F> {}

impl<A, B, F> Signal for MutableSignalRef<A, F> where F: FnMut(&A) -> B {
    type Item = B;

    fn poll_change(mut self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        let state = &this.0;
        let callback = &mut this.1;
        state.poll_change(waker, callback)
    }
}


// TODO it should have a single MutableSignal implementation for both Copy and Clone
// TODO remove it from receivers when it's dropped
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct MutableSignalCloned<A>(Arc<MutableSignalState<A>>);

impl<A> Unpin for MutableSignalCloned<A> {}

impl<A: Clone> Signal for MutableSignalCloned<A> {
    type Item = A;

    // TODO code duplication with MutableSignal::poll
    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        self.0.poll_change(waker, |value| value.clone())
    }
}
