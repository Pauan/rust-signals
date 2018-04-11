use futures_core::task::Context;
use futures_core::{Async, Poll};
use futures_core::future::{Future, IntoFuture};
use futures_core::stream::{Stream};
use futures_util::stream::{StreamExt, ForEach};
use signal_vec::{VecChange, SignalVec};


pub trait IntoSignal {
    type Signal: Signal<Item = Self::Item>;
    type Item;

    fn into_signal(self) -> Self::Signal;
}

impl<A> IntoSignal for A where A: Signal {
    type Signal = Self;
    type Item = A::Item;

    fn into_signal(self) -> Self { self }
}


pub trait Signal {
    type Item;

    fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>>;
}

impl<F: ?Sized + Signal> Signal for ::std::boxed::Box<F> {
    type Item = F::Item;

    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        (**self).poll(cx)
    }
}


pub trait SignalExt: Signal {
    #[inline]
    fn to_stream(self) -> SignalStream<Self>
        where Self: Sized {
        SignalStream {
            signal: self,
        }
    }

    #[inline]
    fn map<A, B>(self, callback: B) -> Map<Self, B>
        where B: FnMut(Self::Item) -> A,
              Self: Sized {
        Map {
            signal: self,
            callback,
        }
    }

    #[inline]
    fn map_dedupe<A, B>(self, callback: B) -> MapDedupe<Self, B>
        // TODO should this use & instead of &mut ?
        where B: FnMut(&mut Self::Item) -> A,
              Self: Sized {
        MapDedupe {
            old_value: None,
            signal: self,
            callback,
        }
    }

    #[inline]
    fn filter_map<A, B>(self, callback: B) -> FilterMap<Self, B>
        where B: FnMut(Self::Item) -> Option<A>,
              Self: Sized {
        FilterMap {
            signal: self,
            callback,
            first: true,
        }
    }

    #[inline]
    fn flatten(self) -> Flatten<Self>
        where Self::Item: IntoSignal,
              Self: Sized {
        Flatten {
            signal: Some(self),
            inner: None,
        }
    }

    #[inline]
    fn switch<A, B>(self, callback: B) -> Switch<Self, A, B>
        where A: IntoSignal,
              B: FnMut(Self::Item) -> A,
              Self: Sized {
        Switch {
            inner: self.map(callback).flatten()
        }
    }

    #[inline]
    // TODO file Rust bug about bad error message when `callback` isn't marked as `mut`
    // TODO custom ForEach type for this
    fn for_each<U, F>(self, callback: F) -> ForEach<SignalStream<Self>, U, F>
        // TODO allow for errors ?
        where U: IntoFuture<Item = (), Error = ()>,
              F: FnMut(Self::Item) -> U,
              Self: Sized {

        self.to_stream().for_each(callback)
    }

    #[inline]
    fn to_signal_vec(self) -> SignalSignalVec<Self>
        where Self: Sized {
        SignalSignalVec {
            signal: self
        }
    }

    #[inline]
    fn wait_for(self, value: Self::Item) -> WaitFor<Self>
        where Self::Item: PartialEq,
              Self: Sized {
        WaitFor {
            signal: self,
            value: value,
        }
    }

    #[inline]
    fn as_mut(&mut self) -> &mut Self where Self: Sized {
        self
    }
}

// TODO why is this ?Sized
impl<T: ?Sized> SignalExt for T where T: Signal {}


pub struct Always<A> {
    value: Option<A>,
}

impl<A> Signal for Always<A> {
    type Item = A;

    #[inline]
    fn poll(&mut self, _: &mut Context) -> Async<Option<Self::Item>> {
        Async::Ready(self.value.take())
    }
}

#[inline]
pub fn always<A>(value: A) -> Always<A> {
    Always {
        value: Some(value),
    }
}


pub struct Switch<A, B, C>
    where A: Signal,
          B: IntoSignal,
          C: FnMut(A::Item) -> B {
    inner: Flatten<Map<A, C>>,
}

impl<A, B, C> Signal for Switch<A, B, C>
    where A: Signal,
          B: IntoSignal,
          C: FnMut(A::Item) -> B {
    type Item = <B::Signal as Signal>::Item;

    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        self.inner.poll(cx)
    }
}


pub struct SignalStream<A> {
    signal: A,
}

impl<A: Signal> Stream for SignalStream<A> {
    type Item = A::Item;
    // TODO use Void instead ?
    type Error = ();

    #[inline]
    fn poll_next(&mut self, cx: &mut Context) -> Poll<Option<Self::Item>, Self::Error> {
        Ok(self.signal.poll(cx))
    }
}


pub struct Map<A, B> {
    signal: A,
    callback: B,
}

impl<A, B, C> Signal for Map<A, B>
    where A: Signal,
          B: FnMut(A::Item) -> C {
    type Item = C;

    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        self.signal.poll(cx).map(|opt| opt.map(|value| (self.callback)(value)))
    }
}


pub struct WaitFor<A>
    where A: Signal,
          A::Item: PartialEq {
    signal: A,
    value: A::Item,
}

impl<A> Future for WaitFor<A>
    where A: Signal,
          A::Item: PartialEq {

    // TODO this should probably return Result<A::Item, A::Item> or Option<A::Item> or something
    type Item = ();
    type Error = ();

    fn poll(&mut self, cx: &mut Context) -> Poll<Self::Item, Self::Error> {
        loop {
            return match self.signal.poll(cx) {
                Async::Ready(Some(value)) => if value == self.value {
                    Ok(Async::Ready(()))

                } else {
                    continue;
                },

                // TODO is this correct ?
                Async::Ready(None) => Err(()),
                Async::Pending => Ok(Async::Pending),
            };
        }
    }
}


pub struct SignalSignalVec<A> {
    signal: A,
}

impl<A, B> SignalVec for SignalSignalVec<A>
    where A: Signal<Item = Vec<B>> {
    type Item = B;

    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Async<Option<VecChange<B>>> {
        self.signal.poll(cx).map(|opt| opt.map(|values| VecChange::Replace { values }))
    }
}


pub struct MapDedupe<A: Signal, B> {
    old_value: Option<A::Item>,
    signal: A,
    callback: B,
}

impl<A, B, C> Signal for MapDedupe<A, B>
    where A: Signal,
          A::Item: PartialEq,
          // TODO should this use & instead of &mut ?
          // TODO should this use Fn instead ?
          B: FnMut(&mut A::Item) -> C {

    type Item = C;

    // TODO should this use #[inline] ?
    fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        loop {
            return match self.signal.poll(cx) {
                Async::Ready(Some(mut value)) => {
                    let has_changed = match self.old_value {
                        Some(ref old_value) => *old_value != value,
                        None => true,
                    };

                    if has_changed {
                        let output = (self.callback)(&mut value);
                        self.old_value = Some(value);
                        Async::Ready(Some(output))

                    } else {
                        continue;
                    }
                },
                Async::Ready(None) => Async::Ready(None),
                Async::Pending => Async::Pending,
            }
        }
    }
}


pub struct FilterMap<A, B> {
    signal: A,
    callback: B,
    first: bool,
}

impl<A, B, C> Signal for FilterMap<A, B>
    where A: Signal,
          B: FnMut(A::Item) -> Option<C> {
    type Item = Option<C>;

    // TODO should this use #[inline] ?
    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        loop {
            return match self.signal.poll(cx) {
                Async::Ready(Some(value)) => match (self.callback)(value) {
                    Some(value) => {
                        self.first = false;
                        Async::Ready(Some(Some(value)))
                    },

                    None => if self.first {
                        self.first = false;
                        Async::Ready(Some(None))

                    } else {
                        continue;
                    },
                },
                Async::Ready(None) => Async::Ready(None),
                Async::Pending => Async::Pending,
            }
        }
    }
}


pub struct Flatten<A: Signal> where A::Item: IntoSignal {
    signal: Option<A>,
    inner: Option<<A::Item as IntoSignal>::Signal>,
}

// Poll parent => Has inner   => Poll inner  => Output
// --------------------------------------------------------
// Some(inner) =>             => Some(value) => Some(value)
// Some(inner) =>             => None        => Pending
// Some(inner) =>             => Pending     => Pending
// None        => Some(inner) => Some(value) => Some(value)
// None        => Some(inner) => None        => None
// None        => Some(inner) => Pending     => Pending
// None        => None        =>             => None
// Pending     => Some(inner) => Some(value) => Some(value)
// Pending     => Some(inner) => None        => Pending
// Pending     => Some(inner) => Pending     => Pending
// Pending     => None        =>             => Pending
impl<A> Signal for Flatten<A>
    where A: Signal,
          A::Item: IntoSignal {
    type Item = <<A as Signal>::Item as IntoSignal>::Item;

    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        let done = match self.signal.as_mut().map(|signal| signal.poll(cx)) {
            None => true,
            Some(Async::Ready(None)) => {
                self.signal = None;
                true
            },
            Some(Async::Ready(Some(inner))) => {
                self.inner = Some(inner.into_signal());
                false
            },
            Some(Async::Pending) => false,
        };

        let poll = match self.inner {
            Some(ref mut inner) => inner.poll(cx),

            None => if done {
                return Async::Ready(None);

            } else {
                return Async::Pending;
            }
        };

        match poll {
            Async::Ready(None) => {
                self.inner = None;

                if done {
                    Async::Ready(None)

                } else {
                    Async::Pending
                }
            },

            a => a,
        }
    }
}


// TODO verify that this is correct
mod mutable {
    use super::Signal;
    use std;
    // TODO use parking_lot ?
    use std::sync::{Arc, Weak, Mutex, RwLock, MutexGuard};
    // TODO use parking_lot ?
    use std::sync::atomic::{AtomicBool, Ordering};
    use futures_core::Async;
    use futures_core::task::{Context, Waker};
    use serde::{Serialize, Deserialize, Serializer, Deserializer};


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
                lock.receivers.push(Arc::downgrade(&state));
            }

            state
        }
    }


    pub struct Mutable<A>(Arc<RwLock<MutableState<A>>>);

    impl<A> Mutable<A> {
        pub fn new(value: A) -> Self {
            Mutable(Arc::new(RwLock::new(MutableState {
                value,
                senders: 1,
                receivers: vec![],
            })))
        }

        pub fn replace(&self, value: A) -> A {
            let mut state = self.0.write().unwrap();

            let value = std::mem::replace(&mut state.value, value);

            state.notify(true);

            value
        }

        pub fn replace_with<F>(&self, f: F) -> A where F: FnOnce(&mut A) -> A {
            let mut state = self.0.write().unwrap();

            let new_value = f(&mut state.value);
            let value = std::mem::replace(&mut state.value, new_value);

            state.notify(true);

            value
        }

        pub fn swap(&self, other: &Mutable<A>) {
            // TODO can this dead lock ?
            let mut state1 = self.0.write().unwrap();
            let mut state2 = other.0.write().unwrap();

            std::mem::swap(&mut state1.value, &mut state2.value);

            state1.notify(true);
            state2.notify(true);
        }

        pub fn set(&self, value: A) {
            let mut state = self.0.write().unwrap();

            state.value = value;

            state.notify(true);
        }

        // TODO figure out a better name for this ?
        pub fn with_ref<B, F>(&self, f: F) -> B where F: FnOnce(&A) -> B {
            let state = self.0.read().unwrap();
            f(&state.value)
        }
    }

    impl<A: Copy> Mutable<A> {
        #[inline]
        pub fn get(&self) -> A {
            self.0.read().unwrap().value
        }

        #[inline]
        pub fn signal(&self) -> MutableSignal<A> {
            MutableSignal(MutableSignalState::new(&self.0))
        }
    }

    impl<A: Clone> Mutable<A> {
        #[inline]
        pub fn get_cloned(&self) -> A {
            self.0.read().unwrap().value.clone()
        }

        #[inline]
        pub fn signal_cloned(&self) -> MutableSignalCloned<A> {
            MutableSignalCloned(MutableSignalState::new(&self.0))
        }
    }

    impl<T> Serialize for Mutable<T> where T: Serialize {
        #[inline]
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
            self.0.read().unwrap().value.serialize(serializer)
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

    /*impl<A> Clone for Mutable<A> {
        #[inline]
        fn clone(&self) -> Self {
            self.0.write().unwrap().senders += 1;
            Mutable(self.0.clone())
        }
    }*/

    impl<A> Drop for Mutable<A> {
        #[inline]
        fn drop(&mut self) {
            let mut state = self.0.write().unwrap();

            state.senders -= 1;

            if state.senders == 0 && state.receivers.len() > 0 {
                state.notify(false);
                state.receivers = vec![];
            }
        }
    }


    // TODO remove it from receivers when it's dropped
    pub struct MutableSignal<A>(Arc<MutableSignalState<A>>);

    impl<A> Clone for MutableSignal<A> {
        #[inline]
        fn clone(&self) -> Self {
            MutableSignal(MutableSignalState::new(&self.0.state))
        }
    }

    impl<A: Copy> Signal for MutableSignal<A> {
        type Item = A;

        fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
            // TODO is this correct ?
            let lock = self.0.state.read().unwrap();

            // TODO verify that this is correct
            if self.0.has_changed.swap(false, Ordering::SeqCst) {
                Async::Ready(Some(lock.value))

            } else if lock.senders == 0 {
                Async::Ready(None)

            } else {
                // TODO is this correct ?
                *self.0.waker.lock().unwrap() = Some(cx.waker().clone());
                Async::Pending
            }
        }
    }


    // TODO it should have a single MutableSignal implementation for both Copy and Clone
    // TODO remove it from receivers when it's dropped
    pub struct MutableSignalCloned<A>(Arc<MutableSignalState<A>>);

    impl<A> Clone for MutableSignalCloned<A> {
        #[inline]
        fn clone(&self) -> Self {
            MutableSignalCloned(MutableSignalState::new(&self.0.state))
        }
    }

    impl<A: Clone> Signal for MutableSignalCloned<A> {
        type Item = A;

        // TODO code duplication with MutableSignal::poll
        fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
            // TODO is this correct ?
            let lock = self.0.state.read().unwrap();

            // TODO verify that this is correct
            if self.0.has_changed.swap(false, Ordering::SeqCst) {
                Async::Ready(Some(lock.value.clone()))

            } else if lock.senders == 0 {
                Async::Ready(None)

            } else {
                // TODO is this correct ?
                *self.0.waker.lock().unwrap() = Some(cx.waker().clone());
                Async::Pending
            }
        }
    }


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
        fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
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
}

pub use self::mutable::*;
