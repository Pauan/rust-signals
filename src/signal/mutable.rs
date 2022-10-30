use super::Signal;
use std;
use std::fmt;
use std::pin::Pin;
use std::marker::Unpin;
use std::ops::{Deref, DerefMut};
// TODO use parking_lot ?
use std::sync::{Arc, Weak, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
// TODO use parking_lot ?
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Poll, Waker, Context};


#[derive(Debug)]
pub(crate) struct ChangedWaker {
    changed: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl ChangedWaker {
    pub(crate) fn new() -> Self {
        Self {
            changed: AtomicBool::new(true),
            waker: Mutex::new(None),
        }
    }

    pub(crate) fn wake(&self, changed: bool) {
        let waker = {
            let mut lock = self.waker.lock().unwrap();

            if changed {
                self.changed.store(true, Ordering::SeqCst);
            }

            lock.take()
        };

        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub(crate) fn set_waker(&self, cx: &Context) {
        *self.waker.lock().unwrap() = Some(cx.waker().clone());
    }

    pub(crate) fn is_changed(&self) -> bool {
        self.changed.swap(false, Ordering::SeqCst)
    }
}


#[derive(Debug)]
struct MutableLockState<A> {
    value: A,
    // TODO use HashMap or BTreeMap instead ?
    signals: Vec<Weak<ChangedWaker>>,
}

impl<A> MutableLockState<A> {
    fn push_signal(&mut self, state: &Arc<ChangedWaker>) {
        self.signals.push(Arc::downgrade(state));
    }

    fn notify(&mut self, has_changed: bool) {
        self.signals.retain(|signal| {
            if let Some(signal) = signal.upgrade() {
                signal.wake(has_changed);
                true

            } else {
                false
            }
        });
    }
}


#[derive(Debug)]
struct MutableState<A> {
    senders: AtomicUsize,
    lock: RwLock<MutableLockState<A>>,
}


#[derive(Debug)]
struct MutableSignalState<A> {
    waker: Arc<ChangedWaker>,
    // TODO change this to Weak ?
    state: Arc<MutableState<A>>,
}

impl<A> MutableSignalState<A> {
    fn new(state: Arc<MutableState<A>>) -> Self {
        MutableSignalState {
            waker: Arc::new(ChangedWaker::new()),
            state,
        }
    }

    fn poll_change<B, F>(&self, cx: &mut Context, f: F) -> Poll<Option<B>> where F: FnOnce(&A) -> B {
        if self.waker.is_changed() {
            let value = {
                let lock = self.state.lock.read().unwrap();
                f(&lock.value)
            };
            Poll::Ready(Some(value))

        } else if self.state.senders.load(Ordering::SeqCst) == 0 {
            Poll::Ready(None)

        } else {
            self.waker.set_waker(cx);
            Poll::Pending
        }
    }
}


#[derive(Debug)]
pub struct MutableLockMut<'a, A> where A: 'a {
    mutated: bool,
    lock: RwLockWriteGuard<'a, MutableLockState<A>>,
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
    lock: RwLockReadGuard<'a, MutableLockState<A>>,
}

impl<'a, A> Deref for MutableLockRef<'a, A> {
    type Target = A;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.lock.value
    }
}


#[repr(transparent)]
pub struct ReadOnlyMutable<A>(Arc<MutableState<A>>);

impl<A> ReadOnlyMutable<A> {
    // TODO return Result ?
    #[inline]
    pub fn lock_ref(&self) -> MutableLockRef<A> {
        MutableLockRef {
            lock: self.0.lock.read().unwrap(),
        }
    }

    fn signal_state(&self) -> MutableSignalState<A> {
        let signal = MutableSignalState::new(self.0.clone());

        if self.0.senders.load(Ordering::SeqCst) != 0 {
            self.0.lock.write().unwrap().push_signal(&signal.waker);
        }

        signal
    }

    #[inline]
    pub fn signal_ref<B, F>(&self, f: F) -> MutableSignalRef<A, F> where F: FnMut(&A) -> B {
        MutableSignalRef(self.signal_state(), f)
    }
}

impl<A: Copy> ReadOnlyMutable<A> {
    #[inline]
    pub fn get(&self) -> A {
        self.0.lock.read().unwrap().value
    }

    #[inline]
    pub fn signal(&self) -> MutableSignal<A> {
        MutableSignal(self.signal_state())
    }
}

impl<A: Clone> ReadOnlyMutable<A> {
    #[inline]
    pub fn get_cloned(&self) -> A {
        self.0.lock.read().unwrap().value.clone()
    }

    #[inline]
    pub fn signal_cloned(&self) -> MutableSignalCloned<A> {
        MutableSignalCloned(self.signal_state())
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
        let state = self.0.lock.read().unwrap();

        fmt.debug_tuple("ReadOnlyMutable")
            .field(&state.value)
            .finish()
    }
}


#[repr(transparent)]
pub struct Mutable<A>(ReadOnlyMutable<A>);

impl<A:Default> Mutable<A> {
    pub fn replace_object_with<F>(&self, f: F) where F: FnOnce(A) -> A {
        let mut state = self.state().lock.write().unwrap();
        let old_value=std::mem::take(&mut state.value);
        let new_value = f(old_value);
        state.value=new_value;
        
        state.notify(true);
    }
}

impl<A> Mutable<A> {
    // TODO should this inline ?
    pub fn new(value: A) -> Self {
        Mutable(ReadOnlyMutable(Arc::new(MutableState {
            senders: AtomicUsize::new(1),
            lock: RwLock::new(MutableLockState {
                value,
                signals: vec![],
            }),
        })))
    }

    #[inline]
    fn state(&self) -> &Arc<MutableState<A>> {
        &(self.0).0
    }

    #[inline]
    pub fn read_only(&self) -> ReadOnlyMutable<A> {
        self.0.clone()
    }

    pub fn replace(&self, value: A) -> A {
        let mut state = self.state().lock.write().unwrap();

        let value = std::mem::replace(&mut state.value, value);

        state.notify(true);

        value
    }

    pub fn replace_with<F>(&self, f: F) -> A where F: FnOnce(&mut A) -> A {
        let mut state = self.state().lock.write().unwrap();

        let new_value = f(&mut state.value);
        let value = std::mem::replace(&mut state.value, new_value);

        state.notify(true);

        value
    }

    pub fn swap(&self, other: &Mutable<A>) {
        // TODO can this dead lock ?
        let mut state1 = self.state().lock.write().unwrap();
        let mut state2 = other.state().lock.write().unwrap();

        std::mem::swap(&mut state1.value, &mut state2.value);

        state1.notify(true);
        state2.notify(true);
    }

    pub fn set(&self, value: A) {
        let mut state = self.state().lock.write().unwrap();

        state.value = value;

        state.notify(true);
    }

    pub fn set_if<F>(&self, value: A, f: F) where F: FnOnce(&A, &A) -> bool {
        let mut state = self.state().lock.write().unwrap();

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
            lock: self.state().lock.write().unwrap(),
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
        let state = self.state().lock.read().unwrap();

        fmt.debug_tuple("Mutable")
            .field(&state.value)
            .finish()
    }
}

#[cfg(feature = "serde")]
impl<T> serde::Serialize for Mutable<T> where T: serde::Serialize {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: serde::Serializer {
        self.state().lock.read().unwrap().value.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de, T> serde::Deserialize<'de> for Mutable<T> where T: serde::Deserialize<'de> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: serde::Deserializer<'de> {
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
        self.state().senders.fetch_add(1, Ordering::SeqCst);
        Mutable(self.0.clone())
    }
}

impl<A> Drop for Mutable<A> {
    #[inline]
    fn drop(&mut self) {
        let state = self.state();

        let old_senders = state.senders.fetch_sub(1, Ordering::SeqCst);

        if old_senders == 1 {
            let mut lock = state.lock.write().unwrap();

            if lock.signals.len() > 0 {
                lock.notify(false);
                // TODO is this necessary ?
                lock.signals = vec![];
            }
        }
    }
}


// TODO remove it from signals when it's dropped
#[derive(Debug)]
#[repr(transparent)]
#[must_use = "Signals do nothing unless polled"]
pub struct MutableSignal<A>(MutableSignalState<A>);

impl<A> Unpin for MutableSignal<A> {}

impl<A: Copy> Signal for MutableSignal<A> {
    type Item = A;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.0.poll_change(cx, |value| *value)
    }
}


// TODO remove it from signals when it's dropped
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct MutableSignalRef<A, F>(MutableSignalState<A>, F);

impl<A, F> Unpin for MutableSignalRef<A, F> {}

impl<A, B, F> Signal for MutableSignalRef<A, F> where F: FnMut(&A) -> B {
    type Item = B;

    fn poll_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        let state = &this.0;
        let callback = &mut this.1;
        state.poll_change(cx, callback)
    }
}


// TODO it should have a single MutableSignal implementation for both Copy and Clone
// TODO remove it from signals when it's dropped
#[derive(Debug)]
#[repr(transparent)]
#[must_use = "Signals do nothing unless polled"]
pub struct MutableSignalCloned<A>(MutableSignalState<A>);

impl<A> Unpin for MutableSignalCloned<A> {}

impl<A: Clone> Signal for MutableSignalCloned<A> {
    type Item = A;

    // TODO code duplication with MutableSignal::poll
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.0.poll_change(cx, |value| value.clone())
    }
}
