// TODO use parking_lot ?
use std::sync::{Arc, Weak, Mutex};
// TODO use parking_lot ?
use std::sync::atomic::{AtomicBool, Ordering};
use futures_core::task::{Context, Waker};
use futures_core::{Async, Poll};
use futures_core::future::{Future, IntoFuture};
use futures_core::stream::{Stream};
use futures_util::stream::{StreamExt, ForEach};
use discard::{Discard, DiscardOnDrop};
use signal_vec::{VecChange, SignalVec};


pub trait Signal {
    type Item;

    fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>>;

    #[inline]
    fn to_stream(self) -> SignalStream<Self>
        where Self: Sized {
        SignalStream {
            signal: self,
        }
    }

    #[inline]
    fn map<A, B>(self, callback: A) -> Map<Self, A>
        where A: FnMut(Self::Item) -> B,
              Self: Sized {
        Map {
            signal: self,
            callback,
        }
    }

    #[inline]
    fn map_dedupe<A, B>(self, callback: A) -> MapDedupe<Self, A>
        // TODO should this use & instead of &mut ?
        where A: FnMut(&mut Self::Item) -> B,
              Self: Sized {
        MapDedupe {
            old_value: None,
            signal: self,
            callback,
        }
    }

    #[inline]
    fn filter_map<A, B>(self, callback: A) -> FilterMap<Self, A>
        where A: FnMut(Self::Item) -> Option<B>,
              Self: Sized {
        FilterMap {
            signal: self,
            callback,
            first: true,
        }
    }

    #[inline]
    fn flatten(self) -> Flatten<Self>
        where Self::Item: Signal,
              Self: Sized {
        Flatten {
            signal: Some(self),
            inner: None,
        }
    }

    #[inline]
    fn switch<A, B>(self, callback: A) -> Flatten<Map<Self, A>>
        where A: FnMut(Self::Item) -> B,
              B: Signal,
              Self: Sized {
        self.map(callback).flatten()
    }

    #[inline]
    // TODO file Rust bug about bad error message when `callback` isn't marked as `mut`
    fn for_each<F, U>(self, callback: F) -> ForEach<SignalStream<Self>, U, F>
        where F: FnMut(Self::Item) -> U,
              // TODO allow for errors ?
              U: IntoFuture<Item = (), Error = ()>,
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


impl<F: ?Sized + Signal> Signal for ::std::boxed::Box<F> {
    type Item = F::Item;

    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        (**self).poll(cx)
    }
}


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
#[inline]
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


pub struct Flatten<A: Signal> {
    signal: Option<A>,
    inner: Option<A::Item>,
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
          A::Item: Signal {
    type Item = <<A as Signal>::Item as Signal>::Item;

    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        let done = match self.signal.as_mut().map(|signal| signal.poll(cx)) {
            None => true,
            Some(Async::Ready(None)) => {
                self.signal = None;
                true
            },
            Some(Async::Ready(Some(inner))) => {
                self.inner = Some(inner);
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
pub mod unsync {
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
        fn notify(&mut self) {
            self.receivers.retain(|receiver| {
                if let Some(receiver) = receiver.upgrade() {
                    let mut lock = receiver.waker.lock().unwrap();

                    // TODO verify that this is correct
                    receiver.has_changed.store(true, Ordering::SeqCst);

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

            state.notify();

            value
        }

        pub fn replace_with<F>(&self, f: F) -> A where F: FnOnce(&mut A) -> A {
            let mut state = self.0.write().unwrap();

            let new_value = f(&mut state.value);
            let value = std::mem::replace(&mut state.value, new_value);

            state.notify();

            value
        }

        pub fn swap(&self, other: &Mutable<A>) {
            // TODO can this dead lock ?
            let mut state1 = self.0.write().unwrap();
            let mut state2 = other.0.write().unwrap();

            std::mem::swap(&mut state1.value, &mut state2.value);

            state1.notify();
            state2.notify();
        }

        pub fn set(&self, value: A) {
            let mut state = self.0.write().unwrap();

            state.value = value;

            state.notify();
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
                state.notify();
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

#[cfg(test)]
mod tests {
    use super::{Signal, cancelable_future};
    use super::unsync::{Mutable, channel};

    use futures_core::{Async, Future};
    use futures_core::future::{FutureResult};
    use futures_core::task::{Context, LocalMap, Waker, Wake};
    use futures_executor::LocalPool;

    use std::sync::Arc;

    fn with_noop_context<U, F: FnOnce(&mut Context) -> U>(f: F) -> U {

        // borrowed this design from the futures source
        struct Noop;

        impl Wake for Noop {
            fn wake(_: &Arc<Self>) {}
        }

        let waker = Waker::from(Arc::new(Noop));

        let pool = LocalPool::new();
        let mut exec = pool.executor();
        let mut map = LocalMap::new();
        let mut cx = Context::new(&mut map, &waker, &mut exec);

        f(&mut cx)
    }

    #[test]
    fn test_mutable() {
        let mutable = Mutable::new(1);
        let mut s = mutable.signal();

        with_noop_context(|cx| {
            assert_eq!(s.poll(cx), Async::Ready(Some(1)));
            assert_eq!(s.poll(cx), Async::Pending);

            mutable.set(5);
            assert_eq!(s.poll(cx), Async::Ready(Some(5)));
            assert_eq!(s.poll(cx), Async::Pending);
        });
    }

    #[test]
    fn test_cancelable_future() {
        let mut a = cancelable_future(Ok(()), |_: FutureResult<(), ()>| ());

        with_noop_context(|cx| {
            assert_eq!(a.1.poll(cx), Ok(Async::Ready(())));
        });
    }

    #[test]
    fn test_send_sync() {
        let a = cancelable_future(Ok(()), |_: FutureResult<(), ()>| ());
        let _: Box<Send + Sync> = Box::new(a.0);
        let _: Box<Send + Sync> = Box::new(a.1);

        let _: Box<Send + Sync> = Box::new(Mutable::new(1));
        let _: Box<Send + Sync> = Box::new(Mutable::new(1).signal());
        let _: Box<Send + Sync> = Box::new(Mutable::new(1).signal_cloned());

        let a = channel(1);
        let _: Box<Send + Sync> = Box::new(a.0);
        let _: Box<Send + Sync> = Box::new(a.1);
    }
}
