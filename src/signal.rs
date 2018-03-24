use std::rc::{Rc, Weak};
use std::cell::{Cell, RefCell};
use futures::{Async, Poll, task};
use futures::future::{Future, IntoFuture};
use futures::stream::{Stream, ForEach};
use discard::{Discard, DiscardOnDrop};
use signal_vec::{VecChange, SignalVec};


// TODO add in Done to allow the Signal to end ?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State<A> {
    Changed(A),
    NotChanged,
}

impl<A> State<A> {
    #[inline]
    pub fn map<B, F>(self, f: F) -> State<B> where F: FnOnce(A) -> B {
        match self {
            State::Changed(value) => State::Changed(f(value)),
            State::NotChanged => State::NotChanged,
        }
    }

    #[inline]
    pub fn unwrap(self) -> A {
        match self {
            State::Changed(value) => value,
            State::NotChanged => panic!("State has not changed!"),
        }
    }
}


pub trait Signal {
    type Item;

    fn poll(&mut self) -> State<Self::Item>;

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
            signal: self,
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
    fn for_each<F, U>(self, callback: F) -> ForEach<SignalStream<Self>, F, U>
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
    fn poll(&mut self) -> State<Self::Item> {
        (**self).poll()
    }
}


pub struct Always<A> {
    value: Option<A>,
}

impl<A> Signal for Always<A> {
    type Item = A;

    #[inline]
    fn poll(&mut self) -> State<Self::Item> {
        match self.value.take() {
            Some(value) => State::Changed(value),
            None => State::NotChanged,
        }
    }
}

#[inline]
pub fn always<A>(value: A) -> Always<A> {
    Always {
        value: Some(value),
    }
}


struct CancelableFutureState {
    is_cancelled: Cell<bool>,
    task: RefCell<Option<task::Task>>,
}


pub struct CancelableFutureHandle {
    state: Weak<CancelableFutureState>,
}

impl Discard for CancelableFutureHandle {
    fn discard(self) {
        if let Some(state) = self.state.upgrade() {
            state.is_cancelled.set(true);

            let mut borrow = state.task.borrow_mut();

            if let Some(task) = borrow.take() {
                drop(borrow);
                task.notify();
            }
        }
    }
}


pub struct CancelableFuture<A, B> {
    state: Rc<CancelableFutureState>,
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
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if self.state.is_cancelled.get() {
            let future = self.future.take().unwrap();
            let callback = self.when_cancelled.take().unwrap();
            // TODO figure out how to call the callback immediately when discard is called, e.g. using two Rc<RefCell<>>
            Ok(Async::Ready(callback(future)))

        } else {
            match self.future.as_mut().unwrap().poll() {
                Ok(Async::NotReady) => {
                    *self.state.task.borrow_mut() = Some(task::current());
                    Ok(Async::NotReady)
                },
                a => a,
            }
        }
    }
}


// TODO figure out a more efficient way to implement this
// TODO this should be implemented in the futures crate
#[inline]
pub fn cancelable_future<A, B>(future: A, when_cancelled: B) -> (DiscardOnDrop<CancelableFutureHandle>, CancelableFuture<A, B>)
    where A: Future,
          B: FnOnce(A) -> A::Item {

    let state = Rc::new(CancelableFutureState {
        is_cancelled: Cell::new(false),
        task: RefCell::new(None),
    });

    let cancel_handle = DiscardOnDrop::new(CancelableFutureHandle {
        state: Rc::downgrade(&state),
    });

    let cancel_future = CancelableFuture {
        state,
        future: Some(future),
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
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        Ok(match self.signal.poll() {
            State::Changed(value) => Async::Ready(Some(value)),
            State::NotChanged => Async::NotReady,
        })
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
    fn poll(&mut self) -> State<Self::Item> {
        self.signal.poll().map(|value| (self.callback)(value))
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

    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            return match self.signal.poll() {
                State::Changed(value) => if value == self.value {
                    Ok(Async::Ready(()))

                } else {
                    continue;
                },

                State::NotChanged => Ok(Async::NotReady),
            }
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
    fn poll(&mut self) -> Async<Option<VecChange<B>>> {
        match self.signal.poll() {
            State::Changed(values) => Async::Ready(Some(VecChange::Replace { values })),
            State::NotChanged => Async::NotReady,
        }
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
    fn poll(&mut self) -> State<Self::Item> {
        loop {
            match self.signal.poll() {
                State::Changed(mut value) => {
                    let has_changed = match self.old_value {
                        Some(ref old_value) => *old_value != value,
                        None => true,
                    };

                    if has_changed {
                        let output = (self.callback)(&mut value);
                        self.old_value = Some(value);
                        return State::Changed(output);
                    }
                },
                State::NotChanged => return State::NotChanged,
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
    fn poll(&mut self) -> State<Self::Item> {
        loop {
            return match self.signal.poll() {
                State::Changed(value) => match (self.callback)(value) {
                    Some(value) => {
                        self.first = false;
                        State::Changed(Some(value))
                    },
                    None => if self.first {
                        self.first = false;
                        State::Changed(None)
                    } else {
                        continue;
                    },
                },
                State::NotChanged => State::NotChanged,
            }
        }
    }
}


pub struct Flatten<A: Signal> {
    signal: A,
    inner: Option<A::Item>,
}

impl<A> Signal for Flatten<A>
    where A: Signal,
          A::Item: Signal {
    type Item = <<A as Signal>::Item as Signal>::Item;

    #[inline]
    fn poll(&mut self) -> State<Self::Item> {
        match self.signal.poll() {
            State::Changed(mut inner) => {
                let poll = inner.poll();
                self.inner = Some(inner);
                poll
            },

            State::NotChanged => match self.inner {
                Some(ref mut inner) => inner.poll(),
                None => State::NotChanged,
            },
        }
    }
}


// TODO verify that this is correct
pub mod unsync {
    use super::{Signal, State};
    use std;
    use std::rc::{Rc, Weak};
    use std::cell::{Cell, RefCell, RefMut, Ref};
    use futures::task;
    use futures::task::Task;
    use serde::{Serialize, Deserialize, Serializer, Deserializer};


    struct MutableState<A> {
        value: A,
        // TODO use HashMap or BTreeMap instead ?
        receivers: Vec<Weak<MutableSignalState<A>>>,
    }

    struct MutableSignalState<A> {
        has_changed: Cell<bool>,
        task: RefCell<Option<Task>>,
        // TODO change this to Weak later
        state: Rc<RefCell<MutableState<A>>>,
    }

    impl<A> MutableSignalState<A> {
        fn new(mutable_state: &Rc<RefCell<MutableState<A>>>) -> Rc<Self> {
            let state = Rc::new(MutableSignalState {
                has_changed: Cell::new(true),
                task: RefCell::new(None),
                state: mutable_state.clone(),
            });

            mutable_state.borrow_mut().receivers.push(Rc::downgrade(&state));

            state
        }
    }


    #[derive(Clone)]
    pub struct Mutable<A>(Rc<RefCell<MutableState<A>>>);

    impl<A> Mutable<A> {
        pub fn new(value: A) -> Self {
            Mutable(Rc::new(RefCell::new(MutableState {
                value,
                receivers: vec![],
            })))
        }

        fn notify(state: &mut RefMut<MutableState<A>>) {
            state.receivers.retain(|receiver| {
                if let Some(receiver) = receiver.upgrade() {
                    receiver.has_changed.set(true);

                    let mut borrow = receiver.task.borrow_mut();

                    if let Some(task) = borrow.take() {
                        drop(borrow);
                        task.notify();
                    }

                    true

                } else {
                    false
                }
            });
        }

        /*pub fn update<F>(&self, f: F) where F: FnOnce(A) -> A {
            let mut state = self.0.borrow_mut();

            state.value = f(state.value);

            Self::notify(&mut state);
        }*/

        pub fn replace(&self, value: A) -> A {
            let mut state = self.0.borrow_mut();

            let value = std::mem::replace(&mut state.value, value);

            Self::notify(&mut state);

            value
        }

        pub fn replace_with<F>(&self, f: F) -> A where F: FnOnce(&mut A) -> A {
            let mut state = self.0.borrow_mut();

            let new_value = f(&mut state.value);
            let value = std::mem::replace(&mut state.value, new_value);

            Self::notify(&mut state);

            value
        }

        pub fn swap(&self, other: &Mutable<A>) {
            let mut state1 = self.0.borrow_mut();
            let mut state2 = other.0.borrow_mut();

            std::mem::swap(&mut state1.value, &mut state2.value);

            Self::notify(&mut state1);
            Self::notify(&mut state2);
        }

        pub fn set(&self, value: A) {
            let mut state = self.0.borrow_mut();

            state.value = value;

            Self::notify(&mut state);
        }

        // TODO should this take &mut self ?
        #[inline]
        pub fn borrow(&self) -> Ref<A> {
            Ref::map(self.0.borrow(), |x| &x.value)
        }
    }

    impl<A: Copy> Mutable<A> {
        #[inline]
        pub fn get(&self) -> A {
            self.0.borrow().value
        }

        #[inline]
        pub fn signal(&self) -> MutableSignal<A> {
            MutableSignal(MutableSignalState::new(&self.0))
        }
    }

    impl<A: Clone> Mutable<A> {
        #[inline]
        pub fn get_cloned(&self) -> A {
            self.0.borrow().value.clone()
        }

        #[inline]
        pub fn signal_cloned(&self) -> MutableSignalClone<A> {
            MutableSignalClone(MutableSignalState::new(&self.0))
        }
    }

    impl<T> Serialize for Mutable<T> where T: Serialize {
        #[inline]
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
            self.0.borrow().value.serialize(serializer)
        }
    }

    impl<'de, T> Deserialize<'de> for Mutable<T> where T: Deserialize<'de> {
        #[inline]
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de> {
            T::deserialize(deserializer).map(Mutable::new)
        }
    }

    impl<T: Default> Default for Mutable<T> {
        #[inline]
        fn default() -> Self {
            Mutable::new(Default::default())
        }
    }


    pub struct MutableSignal<A>(Rc<MutableSignalState<A>>);

    impl<A> Clone for MutableSignal<A> {
        #[inline]
        fn clone(&self) -> Self {
            MutableSignal(MutableSignalState::new(&self.0.state))
        }
    }

    impl<A: Copy> Signal for MutableSignal<A> {
        type Item = A;

        fn poll(&mut self) -> State<Self::Item> {
            if self.0.has_changed.replace(false) {
                State::Changed(self.0.state.borrow().value)

            } else {
                *self.0.task.borrow_mut() = Some(task::current());
                State::NotChanged
            }
        }
    }


    // TODO it should have a single MutableSignal implementation for both Copy and Clone
    pub struct MutableSignalClone<A>(Rc<MutableSignalState<A>>);

    impl<A> Clone for MutableSignalClone<A> {
        #[inline]
        fn clone(&self) -> Self {
            MutableSignalClone(MutableSignalState::new(&self.0.state))
        }
    }

    impl<A: Clone> Signal for MutableSignalClone<A> {
        type Item = A;

        // TODO code duplication with MutableSignal::poll
        fn poll(&mut self) -> State<Self::Item> {
            if self.0.has_changed.replace(false) {
                State::Changed(self.0.state.borrow().value.clone())

            } else {
                *self.0.task.borrow_mut() = Some(task::current());
                State::NotChanged
            }
        }
    }


    struct Inner<A> {
        value: Option<A>,
        task: Option<task::Task>,
    }

    pub struct Sender<A> {
        inner: Weak<RefCell<Inner<A>>>,
    }

    impl<A> Sender<A> {
        pub fn send(&self, value: A) -> Result<(), A> {
            if let Some(inner) = self.inner.upgrade() {
                let mut inner = inner.borrow_mut();

                inner.value = Some(value);

                if let Some(task) = inner.task.take() {
                    drop(inner);
                    task.notify();
                }

                Ok(())

            } else {
                Err(value)
            }
        }
    }

    pub struct Receiver<A> {
        inner: Rc<RefCell<Inner<A>>>,
    }

    impl<A> Signal for Receiver<A> {
        type Item = A;

        #[inline]
        fn poll(&mut self) -> State<Self::Item> {
            let mut inner = self.inner.borrow_mut();

            // TODO is this correct ?
            match inner.value.take() {
                Some(value) => State::Changed(value),
                None => {
                    inner.task = Some(task::current());
                    State::NotChanged
                },
            }
        }
    }

    pub fn channel<A>(initial_value: A) -> (Sender<A>, Receiver<A>) {
        let inner = Rc::new(RefCell::new(Inner {
            value: Some(initial_value),
            task: None,
        }));

        let sender = Sender {
            inner: Rc::downgrade(&inner),
        };

        let receiver = Receiver {
            inner,
        };

        (sender, receiver)
    }
}
