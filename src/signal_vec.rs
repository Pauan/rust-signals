use std::marker::PhantomData;
use std::cmp::Ordering;
use futures_core::task::Context;
use futures_core::{Future, Stream, Poll, Async, Never};
use futures_core::future::IntoFuture;
use futures_util::stream;
use futures_util::stream::StreamExt;
use signal::{Signal, Mutable, MutableSignal};


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VecDiff<A> {
    Replace {
        values: Vec<A>,
    },

    InsertAt {
        index: usize,
        value: A,
    },

    UpdateAt {
        index: usize,
        value: A,
    },

    RemoveAt {
        index: usize,
    },

    // TODO
    /*Batch {
        changes: Vec<VecDiff<A>>,
    }*/

    // TODO
    /*Swap {
        old_index: usize,
        new_index: usize,
    },*/

    Move {
        old_index: usize,
        new_index: usize,
    },

    Push {
        value: A,
    },

    Pop {},

    Clear {},
}

impl<A> VecDiff<A> {
    // TODO inline this ?
    fn map<B, F>(self, mut callback: F) -> VecDiff<B> where F: FnMut(A) -> B {
        match self {
            // TODO figure out a more efficient way of implementing this
            VecDiff::Replace { values } => VecDiff::Replace { values: values.into_iter().map(callback).collect() },
            VecDiff::InsertAt { index, value } => VecDiff::InsertAt { index, value: callback(value) },
            VecDiff::UpdateAt { index, value } => VecDiff::UpdateAt { index, value: callback(value) },
            VecDiff::Push { value } => VecDiff::Push { value: callback(value) },
            VecDiff::RemoveAt { index } => VecDiff::RemoveAt { index },
            VecDiff::Move { old_index, new_index } => VecDiff::Move { old_index, new_index },
            VecDiff::Pop {} => VecDiff::Pop {},
            VecDiff::Clear {} => VecDiff::Clear {},
        }
    }
}


pub trait IntoSignalVec {
    type SignalVec: SignalVec<Item = Self::Item>;
    type Item;

    fn into_signal_vec(self) -> Self::SignalVec;
}

impl<A> IntoSignalVec for A where A: SignalVec {
    type SignalVec = Self;
    type Item = A::Item;

    fn into_signal_vec(self) -> Self { self }
}


pub trait SignalVec {
    type Item;

    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecDiff<Self::Item>>>;
}

impl<F: ?Sized + SignalVec> SignalVec for ::std::boxed::Box<F> {
    type Item = F::Item;

    #[inline]
    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecDiff<Self::Item>>> {
        (**self).poll_vec_change(cx)
    }
}


pub trait SignalVecExt: SignalVec {
    /// Creates a `SignalVec` which uses a closure to transform the values.
    ///
    /// When the output `SignalVec` is spawned:
    ///
    /// 1. It calls the closure once for each value in `self`. The return values from the closure are
    ///    put into the output `SignalVec` in the same order as `self`.
    ///
    /// 2. Whenever `self` changes it calls the closure for the new values, and updates the
    ///    output `SignalVec` as appropriate, maintaining the same order as `self`.
    ///
    /// It is guaranteed that the closure will be called *exactly* once for each value in `self`.
    ///
    /// # Examples
    ///
    /// Add `1` to each value:
    ///
    /// ```rust
    /// # use futures_signals::signal_vec::{always, SignalVecExt};
    /// # let input = always(vec![1, 2, 3, 4, 5]);
    /// let mapped = input.map(|value| value + 1);
    /// ```
    ///
    /// If `input` has the values `[1, 2, 3, 4, 5]` then `mapped` has the values `[2, 3, 4, 5, 6]`
    ///
    /// ----
    ///
    /// Formatting to a `String`:
    ///
    /// ```rust
    /// # use futures_signals::signal_vec::{always, SignalVecExt};
    /// # let input = always(vec![1, 2, 3, 4, 5]);
    /// let mapped = input.map(|value| format!("{}", value));
    /// ```
    ///
    /// If `input` has the values `[1, 2, 3, 4, 5]` then `mapped` has the values `["1", "2", "3", "4", "5"]`
    ///
    /// # Performance
    ///
    /// This is an ***extremely*** efficient method: it is *guaranteed* constant time, regardless of how big `self` is.
    ///
    /// In addition, it does not do any heap allocation, and it doesn't need to maintain any extra internal state.
    ///
    /// The only exception is when `self` notifies with `VecDiff::Replace`, in which case it is linear time
    /// (and it heap allocates a single [`Vec`](https://doc.rust-lang.org/std/vec/struct.Vec.html)).
    #[inline]
    fn map<A, F>(self, callback: F) -> Map<Self, F>
        where F: FnMut(Self::Item) -> A,
              Self: Sized {
        Map {
            signal: self,
            callback,
        }
    }

    #[inline]
    fn map_signal<A, F>(self, callback: F) -> MapSignal<Self, A, F>
        where A: Signal,
              F: FnMut(Self::Item) -> A,
              Self: Sized {
        MapSignal {
            signal: Some(self),
            signals: vec![],
            callback,
        }
    }

    /// Creates a `SignalVec` which uses a closure to determine if a value should be included or not.
    ///
    /// When the output `SignalVec` is spawned:
    ///
    /// 1. It calls the closure once for each value in `self`. The output `SignalVec` contains all
    ///    of the values where the closure returned `true`, in the same order as `self`.
    ///
    /// 2. Whenever `self` changes it calls the closure for the new values, and filters the
    ///    output `SignalVec` as appropriate, maintaining the same order as `self`.
    ///
    /// It is guaranteed that the closure will be called *exactly* once for each value in `self`.
    ///
    /// # Examples
    ///
    /// Only include values less than `5`:
    ///
    /// ```rust
    /// # use futures_signals::signal_vec::{always, SignalVecExt};
    /// # let input = always(vec![3, 1, 6, 2, 0, 4, 5, 8, 9, 7]);
    /// let filtered = input.filter(|value| *value < 5);
    /// ```
    ///
    /// If `input` has the values `[3, 1, 6, 2, 0, 4, 5, 8, 9, 7]` then `filtered` has the values `[3, 1, 2, 0, 4]`
    ///
    /// # Performance
    ///
    /// The performance is linear with the number of values in `self` (it's the same algorithmic
    /// performance as [`Vec`](https://doc.rust-lang.org/std/vec/struct.Vec.html)).
    ///
    /// As an example, if `self` has 1,000 values and a new value is inserted, `filter` will require (on
    /// average) 1,000 operations to update its internal state. It does ***not*** call the closure while updating
    /// its internal state.
    ///
    /// That might sound expensive, but each individual operation is ***extremely*** fast, so it's normally not a problem
    /// unless `self` is ***really*** huge.
    #[inline]
    fn filter<F>(self, callback: F) -> Filter<Self, F>
        where F: FnMut(&Self::Item) -> bool,
              Self: Sized {
        Filter {
            indexes: vec![],
            signal: self,
            callback,
        }
    }

    /// Creates a `SignalVec` which uses a closure to sort the values.
    ///
    /// When the output `SignalVec` is spawned:
    ///
    /// 1. It repeatedly calls the closure with two different values from `self`, and the closure
    ///    must return an [`Ordering`](https://doc.rust-lang.org/std/cmp/enum.Ordering.html),
    ///    which is used to sort the values. The output `SignalVec` then contains the values in
    ///    sorted order.
    ///
    /// 2. Whenever `self` changes it calls the closure repeatedly, and sorts the
    ///    output `SignalVec` based upon the [`Ordering`](https://doc.rust-lang.org/std/cmp/enum.Ordering.html).
    ///
    /// This method is intentionally very similar to the [`slice::sort_by`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by)
    /// method, except it doesn't mutate `self` (it returns a new `SignalVec`).
    ///
    /// Just like [`slice::sort_by`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by), the
    /// sorting is *stable*: if the closure returns `Ordering::Equal`, then the order will be based upon the
    /// order in `self`.
    ///
    /// The reason why it has the `_cloned` suffix is because it calls [`clone`](https://doc.rust-lang.org/std/clone/trait.Clone.html#tymethod.clone)
    /// on the values from `self`. This is necessary in order to maintain its internal state
    /// while also simultaneously passing the values to the output `SignalVec`.
    ///
    /// You can avoid the cost of cloning by using `.map(Rc::new)` or `.map(Arc::new)` to wrap the values in
    /// [`Rc`](https://doc.rust-lang.org/std/rc/struct.Rc.html) or [`Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html),
    /// like this:
    ///
    /// ```rust
    /// # use futures_signals::signal_vec::{always, SignalVecExt};
    /// # let input = always(vec![3, 1, 6, 2, 0, 4, 5, 8, 9, 7]);
    /// use std::rc::Rc;
    ///
    /// let sorted = input.map(Rc::new).sort_by_cloned(Ord::cmp);
    /// ```
    ///
    /// However, this heap allocates each individual value, so it should only be done when the cost of cloning
    /// is expensive. You should benchmark and profile so you know which one is faster for *your* particular program!
    ///
    /// # Examples
    ///
    /// Sort using the standard [`Ord`](https://doc.rust-lang.org/std/cmp/trait.Ord.html) implementation:
    ///
    /// ```rust
    /// # use futures_signals::signal_vec::{always, SignalVecExt};
    /// # let input = always(vec![3, 1, 6, 2, 0, 4, 5, 8, 9, 7]);
    /// let sorted = input.sort_by_cloned(Ord::cmp);
    /// ```
    ///
    /// If `input` has the values `[3, 1, 6, 2, 0, 4, 5, 8, 9, 7]` then `sorted` has the values `[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]`
    ///
    /// ----
    ///
    /// Sort using a custom function:
    ///
    /// ```rust
    /// # use futures_signals::signal_vec::{always, SignalVecExt};
    /// # let input = always(vec![3, 1, 6, 2, 0, 4, 5, 8, 9, 7]);
    /// let sorted = input.sort_by_cloned(|left, right| left.cmp(right).reverse());
    /// ```
    ///
    /// If `input` has the values `[3, 1, 6, 2, 0, 4, 5, 8, 9, 7]` then `sorted` has the values `[9, 8, 7, 6, 5, 4, 3, 2, 1, 0]`
    ///
    /// # Performance
    ///
    /// It has the same logarithmic performance as [`slice::sort_by`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by),
    /// except it's slower because it needs to keep track of extra internal state.
    ///
    /// As an example, if `self` has 1,000 values and a new value is inserted, then `sort_by_cloned` will require
    /// (on average) ~2,010 operations to update its internal state. It does ***not*** call the closure while updating
    /// its internal state.
    ///
    /// That might sound expensive, but each individual operation is ***extremely*** fast, so it's normally not a problem
    /// unless `self` is ***really*** huge.
    #[inline]
    fn sort_by_cloned<F>(self, compare: F) -> SortByCloned<Self, F>
        where F: FnMut(&Self::Item, &Self::Item) -> Ordering,
              Self: Sized {
        SortByCloned {
            pending: None,
            values: vec![],
            indexes: vec![],
            signal: self,
            compare,
        }
    }

    #[inline]
    fn to_stream(self) -> SignalVecStream<Self, Never> where Self: Sized {
        SignalVecStream {
            signal: self,
            phantom: PhantomData,
        }
    }

    #[inline]
    // TODO file Rust bug about bad error message when `callback` isn't marked as `mut`
    fn for_each<U, F>(self, callback: F) -> ForEach<Self, U, F>
        where U: IntoFuture<Item = ()>,
              F: FnMut(VecDiff<Self::Item>) -> U,
              Self: Sized {
        // TODO a little hacky
        ForEach {
            inner: SignalVecStream {
                signal: self,
                phantom: PhantomData,
            }.for_each(callback)
        }
    }

    #[inline]
    fn len(self) -> Len<Self> where Self: Sized {
        Len {
            signal: Some(self),
            first: true,
            len: 0,
        }
    }

    #[inline]
    fn enumerate(self) -> Enumerate<Self> where Self: Sized {
        Enumerate {
            signal: self,
            mutables: vec![],
        }
    }

    #[inline]
    fn delay_remove<A, F>(self, f: F) -> DelayRemove<Self, A, F>
        where A: Future<Item = (), Error = Never>,
              F: FnMut(&Self::Item) -> A,
              Self: Sized {
        DelayRemove {
            signal: Some(self),
            futures: vec![],
            callback: f,
        }
    }
}

// TODO why is this ?Sized
impl<T: ?Sized> SignalVecExt for T where T: SignalVec {}


pub struct Always<A> {
    values: Option<Vec<A>>,
}

impl<A> SignalVec for Always<A> {
    type Item = A;

    fn poll_vec_change(&mut self, _cx: &mut Context) -> Async<Option<VecDiff<Self::Item>>> {
        Async::Ready(self.values.take().map(|values| VecDiff::Replace { values }))
    }
}

#[inline]
pub fn always<A>(values: Vec<A>) -> Always<A> {
    Always {
        values: Some(values),
    }
}


pub struct ForEach<A, B, C> where B: IntoFuture {
    inner: stream::ForEach<SignalVecStream<A, B::Error>, B, C>
}

impl<A, B, C> Future for ForEach<A, B, C>
    where A: SignalVec,
          B: IntoFuture<Item = ()>,
          C: FnMut(VecDiff<A::Item>) -> B {
    type Item = ();
    type Error = B::Error;

    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Poll<Self::Item, Self::Error> {
        // TODO a teensy bit hacky
        self.inner.poll(cx).map(|async| async.map(|_| ()))
    }
}


pub struct Map<A, B> {
    signal: A,
    callback: B,
}

impl<A, B, F> SignalVec for Map<A, F>
    where A: SignalVec,
          F: FnMut(A::Item) -> B {
    type Item = B;

    // TODO should this inline ?
    #[inline]
    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecDiff<Self::Item>>> {
        self.signal.poll_vec_change(cx).map(|some| some.map(|change| change.map(|value| (self.callback)(value))))
    }
}


#[derive(Clone)]
pub struct MutableIndex(Mutable<Option<usize>>);

impl MutableIndex {
    fn new(value: usize) -> Self {
        MutableIndex(Mutable::new(Some(value)))
    }

    #[inline]
    pub fn get(&self) -> Option<usize> {
        self.0.get()
    }

    // TODO use custom type for this ?
    #[inline]
    pub fn signal(&self) -> MutableSignal<Option<usize>> {
        self.0.signal()
    }
}


pub struct Enumerate<A> {
    signal: A,
    mutables: Vec<MutableIndex>,
}

impl<A> SignalVec for Enumerate<A> where A: SignalVec {
    type Item = (MutableIndex, A::Item);

    #[inline]
    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecDiff<Self::Item>>> {
        fn increment_indexes(range: &[MutableIndex]) {
            for mutable in range {
                mutable.0.replace_with(|value| value.map(|value| value + 1));
            }
        }

        fn decrement_indexes(range: &[MutableIndex]) {
            for mutable in range {
                mutable.0.replace_with(|value| value.map(|value| value - 1));
            }
        }

        // TODO use map ?
        match self.signal.poll_vec_change(cx) {
            Async::Ready(Some(change)) => Async::Ready(Some(match change {
                VecDiff::Replace { values } => {
                    for mutable in self.mutables.drain(..) {
                        mutable.0.set(None);
                    }

                    self.mutables = Vec::with_capacity(values.len());

                    VecDiff::Replace {
                        values: values.into_iter().enumerate().map(|(index, value)| {
                            let mutable = MutableIndex::new(index);
                            self.mutables.push(mutable.clone());
                            (mutable, value)
                        }).collect()
                    }
                },

                VecDiff::InsertAt { index, value } => {
                    let mutable = MutableIndex::new(index);

                    self.mutables.insert(index, mutable.clone());

                    increment_indexes(&self.mutables[(index + 1)..]);

                    VecDiff::InsertAt { index, value: (mutable, value) }
                },

                VecDiff::UpdateAt { index, value } => {
                    VecDiff::UpdateAt { index, value: (self.mutables[index].clone(), value) }
                },

                VecDiff::Push { value } => {
                    let mutable = MutableIndex::new(self.mutables.len());

                    self.mutables.push(mutable.clone());

                    VecDiff::Push { value: (mutable, value) }
                },

                VecDiff::Move { old_index, new_index } => {
                    let mutable = self.mutables.remove(old_index);

                    // TODO figure out a way to avoid this clone ?
                    self.mutables.insert(new_index, mutable.clone());

                    // TODO test this
                    if old_index < new_index {
                        decrement_indexes(&self.mutables[old_index..new_index]);

                    } else if new_index < old_index {
                        increment_indexes(&self.mutables[(new_index + 1)..(old_index + 1)]);
                    }

                    mutable.0.set(Some(new_index));

                    VecDiff::Move { old_index, new_index }
                },

                VecDiff::RemoveAt { index } => {
                    let mutable = self.mutables.remove(index);

                    decrement_indexes(&self.mutables[index..]);

                    mutable.0.set(None);

                    VecDiff::RemoveAt { index }
                },

                VecDiff::Pop {} => {
                    let mutable = self.mutables.pop().unwrap();

                    mutable.0.set(None);

                    VecDiff::Pop {}
                },

                VecDiff::Clear {} => {
                    for mutable in self.mutables.drain(..) {
                        mutable.0.set(None);
                    }

                    VecDiff::Clear {}
                },
            })),
            Async::Ready(None) => Async::Ready(None),
            Async::Pending => Async::Pending,
        }
    }
}


fn unwrap<A>(x: Async<Option<A>>) -> A {
    match x {
        Async::Ready(Some(x)) => x,
        _ => panic!("Signal did not return a value"),
    }
}

pub struct MapSignal<A, B: Signal, F> {
    signal: Option<A>,
    signals: Vec<Option<B>>,
    callback: F,
}

impl<A, B, F> SignalVec for MapSignal<A, B, F>
    where A: SignalVec,
          B: Signal,
          F: FnMut(A::Item) -> B {
    type Item = B::Item;

    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecDiff<Self::Item>>> {
        let done = match self.signal.as_mut().map(|signal| signal.poll_vec_change(cx)) {
            None => true,
            Some(Async::Ready(None)) => {
                self.signal = None;
                true
            },
            Some(Async::Ready(Some(change))) => {
                return Async::Ready(Some(match change {
                    VecDiff::Replace { values } => {
                        self.signals = Vec::with_capacity(values.len());

                        VecDiff::Replace {
                            values: values.into_iter().map(|value| {
                                let mut signal = (self.callback)(value);
                                let poll = unwrap(signal.poll_change(cx));
                                self.signals.push(Some(signal));
                                poll
                            }).collect()
                        }
                    },

                    VecDiff::InsertAt { index, value } => {
                        let mut signal = (self.callback)(value);
                        let poll = unwrap(signal.poll_change(cx));
                        self.signals.insert(index, Some(signal));
                        VecDiff::InsertAt { index, value: poll }
                    },

                    VecDiff::UpdateAt { index, value } => {
                        let mut signal = (self.callback)(value);
                        let poll = unwrap(signal.poll_change(cx));
                        self.signals[index] = Some(signal);
                        VecDiff::UpdateAt { index, value: poll }
                    },

                    VecDiff::Push { value } => {
                        let mut signal = (self.callback)(value);
                        let poll = unwrap(signal.poll_change(cx));
                        self.signals.push(Some(signal));
                        VecDiff::Push { value: poll }
                    },

                    VecDiff::Move { old_index, new_index } => {
                        let value = self.signals.remove(old_index);
                        self.signals.insert(new_index, value);
                        VecDiff::Move { old_index, new_index }
                    },

                    VecDiff::RemoveAt { index } => {
                        self.signals.remove(index);
                        VecDiff::RemoveAt { index }
                    },

                    VecDiff::Pop {} => {
                        self.signals.pop().unwrap();
                        VecDiff::Pop {}
                    },

                    VecDiff::Clear {} => {
                        self.signals.clear();
                        VecDiff::Clear {}
                    },
                }));
            },
            Some(Async::Pending) => false,
        };

        // TODO make this more efficient (e.g. using a similar strategy as FuturesUnordered)
        let mut iter = self.signals.as_mut_slice().into_iter().enumerate();

        let mut has_pending = false;

        // TODO ensure that this is as efficient as possible
        // TODO make this more efficient (e.g. using a similar strategy as FuturesUnordered)
        loop {
            match iter.next() {
                Some((index, signal)) => match signal.as_mut().map(|s| s.poll_change(cx)) {
                    Some(Async::Ready(Some(value))) => {
                        return Async::Ready(Some(VecDiff::UpdateAt { index, value }))
                    },
                    Some(Async::Ready(None)) => {
                        *signal = None;
                    },
                    Some(Async::Pending) => {
                        has_pending = true;
                    },
                    None => {},
                },
                None => return if done && !has_pending {
                    Async::Ready(None)

                } else {
                    Async::Pending
                },
            }
        }
    }
}


pub struct Len<A> {
    signal: Option<A>,
    first: bool,
    len: usize,
}

impl<A> Signal for Len<A> where A: SignalVec {
    type Item = usize;

    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        let mut changed = false;
        let mut done = false;

        loop {
            match self.signal.as_mut().map(|signal| signal.poll_vec_change(cx)) {
                None => {
                    done = true;
                    break;
                },
                Some(Async::Ready(None)) => {
                    self.signal = None;
                    done = true;
                    break;
                },
                Some(Async::Ready(Some(change))) => match change {
                    VecDiff::Replace { values } => {
                        let len = values.len();

                        if self.len != len {
                            self.len = len;
                            changed = true;
                        }
                    },

                    VecDiff::InsertAt { .. } | VecDiff::Push { .. } => {
                        self.len += 1;
                        changed = true;
                    },

                    VecDiff::UpdateAt { .. } | VecDiff::Move { .. } => {},

                    VecDiff::RemoveAt { .. } | VecDiff::Pop {} => {
                        self.len -= 1;
                        changed = true;
                    },

                    VecDiff::Clear {} => {
                        if self.len != 0 {
                            self.len = 0;
                            changed = true;
                        }
                    },
                },
                Some(Async::Pending) => {
                    break;
                },
            }
        }

        if changed || self.first {
            self.first = false;
            Async::Ready(Some(self.len))

        } else if done {
            Async::Ready(None)

        } else {
            Async::Pending
        }
    }
}


pub struct SignalVecStream<A, Error> {
    signal: A,
    phantom: PhantomData<Error>,
}

impl<A: SignalVec, Error> Stream for SignalVecStream<A, Error> {
    type Item = VecDiff<A::Item>;
    type Error = Error;

    #[inline]
    fn poll_next(&mut self, cx: &mut Context) -> Poll<Option<Self::Item>, Self::Error> {
        Ok(self.signal.poll_vec_change(cx))
    }
}


pub struct Filter<A, B> {
    // TODO use a bit vec for smaller size
    indexes: Vec<bool>,
    signal: A,
    callback: B,
}

impl<A, B> Filter<A, B> {
    fn find_index(&self, index: usize) -> usize {
        self.indexes[0..index].into_iter().filter(|x| **x).count()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.indexes.iter().filter(|x| **x).count()
    }
}

impl<A, F> SignalVec for Filter<A, F>
    where A: SignalVec,
          F: FnMut(&A::Item) -> bool {
    type Item = A::Item;

    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecDiff<Self::Item>>> {
        loop {
            return match self.signal.poll_vec_change(cx) {
                Async::Pending => Async::Pending,
                Async::Ready(None) => Async::Ready(None),
                Async::Ready(Some(change)) => match change {
                    VecDiff::Replace { values } => {
                        self.indexes = Vec::with_capacity(values.len());

                        Async::Ready(Some(VecDiff::Replace {
                            values: values.into_iter().filter(|value| {
                                let keep = (self.callback)(value);
                                self.indexes.push(keep);
                                keep
                            }).collect()
                        }))
                    },

                    VecDiff::InsertAt { index, value } => {
                        if (self.callback)(&value) {
                            self.indexes.insert(index, true);
                            Async::Ready(Some(VecDiff::InsertAt { index: self.find_index(index), value }))

                        } else {
                            self.indexes.insert(index, false);
                            continue;
                        }
                    },

                    VecDiff::UpdateAt { index, value } => {
                        if (self.callback)(&value) {
                            if self.indexes[index] {
                                Async::Ready(Some(VecDiff::UpdateAt { index: self.find_index(index), value }))

                            } else {
                                self.indexes[index] = true;
                                Async::Ready(Some(VecDiff::InsertAt { index: self.find_index(index), value }))
                            }

                        } else {
                            if self.indexes[index] {
                                self.indexes[index] = false;
                                Async::Ready(Some(VecDiff::RemoveAt { index: self.find_index(index) }))

                            } else {
                                continue;
                            }
                        }
                    },

                    // TODO unit tests for this
                    VecDiff::Move { old_index, new_index } => {
                        if self.indexes.remove(old_index) {
                            self.indexes.insert(new_index, true);

                            Async::Ready(Some(VecDiff::Move {
                                old_index: self.find_index(old_index),
                                new_index: self.find_index(new_index),
                            }))

                        } else {
                            self.indexes.insert(new_index, false);
                            continue;
                        }
                    },

                    VecDiff::RemoveAt { index } => {
                        if self.indexes.remove(index) {
                            Async::Ready(Some(VecDiff::RemoveAt { index: self.find_index(index) }))

                        } else {
                            continue;
                        }
                    },

                    VecDiff::Push { value } => {
                        if (self.callback)(&value) {
                            self.indexes.push(true);
                            Async::Ready(Some(VecDiff::Push { value }))

                        } else {
                            self.indexes.push(false);
                            continue;
                        }
                    },

                    VecDiff::Pop {} => {
                        if self.indexes.pop().expect("Cannot pop from empty vec") {
                            Async::Ready(Some(VecDiff::Pop {}))

                        } else {
                            continue;
                        }
                    },

                    VecDiff::Clear {} => {
                        self.indexes.clear();
                        Async::Ready(Some(VecDiff::Clear {}))
                    },
                },
            }
        }
    }
}


pub struct SortByCloned<A: SignalVec, B> {
    pending: Option<Async<Option<VecDiff<A::Item>>>>,
    values: Vec<A::Item>,
    indexes: Vec<usize>,
    signal: A,
    compare: B,
}

impl<A, F> SortByCloned<A, F>
    where A: SignalVec,
          F: FnMut(&A::Item, &A::Item) -> Ordering {
    // TODO should this inline ?
    fn binary_search(&mut self, index: usize) -> Result<usize, usize> {
        let compare = &mut self.compare;
        let values = &self.values;
        let value = &values[index];

        // TODO use get_unchecked ?
        self.indexes.binary_search_by(|i| compare(&values[*i], value).then_with(|| i.cmp(&index)))
    }

    fn binary_search_insert(&mut self, index: usize) -> usize {
        match self.binary_search(index) {
            Ok(_) => panic!("Value already exists"),
            Err(new_index) => new_index,
        }
    }

    fn binary_search_remove(&mut self, index: usize) -> usize {
        self.binary_search(index).expect("Could not find value")
    }

    fn increment_indexes(&mut self, start: usize) {
        for index in &mut self.indexes {
            let i = *index;

            if i >= start {
                *index = i + 1;
            }
        }
    }

    fn decrement_indexes(&mut self, start: usize) {
        for index in &mut self.indexes {
            let i = *index;

            if i > start {
                *index = i - 1;
            }
        }
    }

    fn insert_at(&mut self, sorted_index: usize, index: usize, value: A::Item) -> Async<Option<VecDiff<A::Item>>> {
        if sorted_index == self.indexes.len() {
            self.indexes.push(index);

            Async::Ready(Some(VecDiff::Push {
                value,
            }))

        } else {
            self.indexes.insert(sorted_index, index);

            Async::Ready(Some(VecDiff::InsertAt {
                index: sorted_index,
                value,
            }))
        }
    }

    fn remove_at(&mut self, sorted_index: usize) -> Async<Option<VecDiff<A::Item>>> {
        if sorted_index == (self.indexes.len() - 1) {
            self.indexes.pop();

            Async::Ready(Some(VecDiff::Pop {}))

        } else {
            self.indexes.remove(sorted_index);

            Async::Ready(Some(VecDiff::RemoveAt {
                index: sorted_index,
            }))
        }
    }
}

// TODO implementation of this for Copy
impl<A, F> SignalVec for SortByCloned<A, F>
    where A: SignalVec,
          F: FnMut(&A::Item, &A::Item) -> Ordering,
          A::Item: Clone {
    type Item = A::Item;

    // TODO figure out a faster implementation of this
    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecDiff<Self::Item>>> {
        match self.pending.take() {
            Some(value) => value,
            None => loop {
                return match self.signal.poll_vec_change(cx) {
                    Async::Pending => Async::Pending,
                    Async::Ready(None) => Async::Ready(None),
                    Async::Ready(Some(change)) => match change {
                        VecDiff::Replace { mut values } => {
                            // TODO can this be made faster ?
                            let mut indexes: Vec<usize> = (0..values.len()).collect();

                            // TODO use get_unchecked ?
                            indexes.sort_unstable_by(|a, b| (self.compare)(&values[*a], &values[*b]).then_with(|| a.cmp(b)));

                            let output = Async::Ready(Some(VecDiff::Replace {
                                // TODO use get_unchecked ?
                                values: indexes.iter().map(|i| values[*i].clone()).collect()
                            }));

                            self.values = values;
                            self.indexes = indexes;

                            output
                        },

                        VecDiff::InsertAt { index, value } => {
                            let new_value = value.clone();

                            self.values.insert(index, value);

                            self.increment_indexes(index);

                            let sorted_index = self.binary_search_insert(index);

                            self.insert_at(sorted_index, index, new_value)
                        },

                        VecDiff::Push { value } => {
                            let new_value = value.clone();

                            let index = self.values.len();

                            self.values.push(value);

                            let sorted_index = self.binary_search_insert(index);

                            self.insert_at(sorted_index, index, new_value)
                        },

                        VecDiff::UpdateAt { index, value } => {
                            let old_index = self.binary_search_remove(index);

                            let old_output = self.remove_at(old_index);

                            let new_value = value.clone();

                            self.values[index] = value;

                            let new_index = self.binary_search_insert(index);

                            if old_index == new_index {
                                self.indexes.insert(new_index, index);

                                Async::Ready(Some(VecDiff::UpdateAt {
                                    index: new_index,
                                    value: new_value,
                                }))

                            } else {
                                let new_output = self.insert_at(new_index, index, new_value);
                                self.pending = Some(new_output);

                                old_output
                            }
                        },

                        VecDiff::RemoveAt { index } => {
                            let sorted_index = self.binary_search_remove(index);

                            self.values.remove(index);

                            self.decrement_indexes(index);

                            self.remove_at(sorted_index)
                        },

                        // TODO can this be made more efficient ?
                        VecDiff::Move { old_index, new_index } => {
                            let old_sorted_index = self.binary_search_remove(old_index);

                            let value = self.values.remove(old_index);

                            self.decrement_indexes(old_index);

                            self.indexes.remove(old_sorted_index);

                            self.values.insert(new_index, value);

                            self.increment_indexes(new_index);

                            let new_sorted_index = self.binary_search_insert(new_index);

                            self.indexes.insert(new_sorted_index, new_index);

                            if old_sorted_index == new_sorted_index {
                                continue;

                            } else {
                                Async::Ready(Some(VecDiff::Move {
                                    old_index: old_sorted_index,
                                    new_index: new_sorted_index,
                                }))
                            }
                        },

                        VecDiff::Pop {} => {
                            let index = self.values.len() - 1;

                            let sorted_index = self.binary_search_remove(index);

                            self.values.pop();

                            self.remove_at(sorted_index)
                        },

                        VecDiff::Clear {} => {
                            self.values.clear();
                            self.indexes.clear();
                            Async::Ready(Some(VecDiff::Clear {}))
                        },
                    },
                }
            },
        }
    }
}


struct DelayRemoveState<A> {
    future: A,
    is_removing: bool,
}

impl<A> DelayRemoveState<A> {
    #[inline]
    fn new(future: A) -> Self {
        Self {
            future,
            is_removing: false,
        }
    }
}

pub struct DelayRemove<A, B, F> {
    signal: Option<A>,
    futures: Vec<DelayRemoveState<B>>,
    callback: F,
}

impl<S, A, F> DelayRemove<S, A, F>
    where S: SignalVec,
          A: Future<Item = (), Error = Never>,
          F: FnMut(&S::Item) -> A {

    fn remove_index(&mut self, index: usize) -> Async<Option<VecDiff<S::Item>>> {
        if index == (self.futures.len() - 1) {
            self.futures.pop();
            Async::Ready(Some(VecDiff::Pop {}))

        } else {
            self.futures.remove(index);
            Async::Ready(Some(VecDiff::RemoveAt { index }))
        }
    }

    fn should_remove(&mut self, cx: &mut Context, index: usize) -> bool {
        let state = &mut self.futures[index];

        assert!(!state.is_removing);

        if state.future.poll(cx).unwrap().is_ready() {
            true

        } else {
            state.is_removing = true;
            false
        }
    }

    fn find_index(&self, parent_index: usize) -> Option<usize> {
        let mut seen = 0;

        // TODO is there a combinator that can simplify this ?
        self.futures.iter().position(|state| {
            if state.is_removing {
                false

            } else if seen == parent_index {
                true

            } else {
                seen += 1;
                false
            }
        })
    }

    fn find_last_index(&self) -> Option<usize> {
        self.futures.iter().rposition(|state| !state.is_removing)
    }
}

impl<S, A, F> SignalVec for DelayRemove<S, A, F>
    where S: SignalVec,
          A: Future<Item = (), Error = Never>,
          F: FnMut(&S::Item) -> A {
    type Item = S::Item;

    // TODO this can probably be implemented more efficiently
    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecDiff<Self::Item>>> {
        let mut is_done = true;

        // TODO is this loop correct ?
        while let Some(result) = self.signal.as_mut().map(|signal| signal.poll_vec_change(cx)) {
            match result {
                Async::Ready(Some(change)) => return match change {
                    VecDiff::Replace { values } => {
                        // TODO handle old futures
                        self.futures = values.iter().map(|value| DelayRemoveState::new((self.callback)(value))).collect();

                        Async::Ready(Some(VecDiff::Replace { values }))
                    },

                    VecDiff::InsertAt { index, value } => {
                        let index = self.find_index(index).unwrap_or_else(|| self.futures.len());
                        let state = DelayRemoveState::new((self.callback)(&value));
                        self.futures.insert(index, state);
                        Async::Ready(Some(VecDiff::InsertAt { index, value }))
                    },

                    VecDiff::Push { value } => {
                        let state = DelayRemoveState::new((self.callback)(&value));
                        self.futures.push(state);
                        Async::Ready(Some(VecDiff::Push { value }))
                    },

                    VecDiff::UpdateAt { index, value } => {
                        // TODO what about removing the old future ?
                        let index = self.find_index(index).expect("Could not find value");
                        let state = DelayRemoveState::new((self.callback)(&value));
                        self.futures[index] = state;
                        Async::Ready(Some(VecDiff::UpdateAt { index, value }))
                    },

                    // TODO test this
                    // TODO should this be treated as a removal + insertion ?
                    VecDiff::Move { old_index, new_index } => {
                        let old_index = self.find_index(old_index).expect("Could not find value");

                        let state = self.futures.remove(old_index);

                        let new_index = self.find_index(new_index).unwrap_or_else(|| self.futures.len());

                        self.futures.insert(new_index, state);

                        Async::Ready(Some(VecDiff::Move { old_index, new_index }))
                    },

                    VecDiff::RemoveAt { index } => {
                        let index = self.find_index(index).expect("Could not find value");

                        if self.should_remove(cx, index) {
                            self.remove_index(index)

                        } else {
                            continue;
                        }
                    },

                    VecDiff::Pop {} => {
                        let index = self.find_last_index().expect("Cannot pop from empty vec");

                        if self.should_remove(cx, index) {
                            self.remove_index(index)

                        } else {
                            continue;
                        }
                    },

                    // TODO maybe it should play remove animation for this ?
                    VecDiff::Clear {} => {
                        self.futures.clear();
                        Async::Ready(Some(VecDiff::Clear {}))
                    },
                },
                Async::Ready(None) => {
                    self.signal = None;
                    break;
                },
                Async::Pending => {
                    is_done = false;
                    break;
                },
            }
        }

        let mut is_removing = false;

        // TODO make this more efficient (e.g. using a similar strategy as FuturesUnordered)
        // This uses rposition so that way it will return VecDiff::Pop in more situations
        let index = self.futures.iter_mut().rposition(|state| {
            if state.is_removing {
                is_removing = true;
                state.future.poll(cx).unwrap().is_ready()

            } else {
                false
            }
        });

        match index {
            Some(index) => {
                self.remove_index(index)
            },
            None => if is_done && !is_removing {
                Async::Ready(None)

            } else {
                Async::Pending
            },
        }
    }
}


// TODO verify that this is correct
mod mutable_vec {
    use super::{SignalVec, VecDiff};
    use std::ops::Deref;
    use std::sync::{Arc, RwLock, RwLockReadGuard};
    use futures_channel::mpsc;
    use futures_core::{Async, Stream};
    use futures_core::task::Context;
    use serde::{Serialize, Deserialize, Serializer, Deserializer};


    struct MutableVecState<A> {
        values: Vec<A>,
        senders: Vec<mpsc::UnboundedSender<VecDiff<A>>>,
    }

    impl<A> MutableVecState<A> {
        // TODO should this inline ?
        #[inline]
        fn notify<B: FnMut() -> VecDiff<A>>(&mut self, mut change: B) {
            self.senders.retain(|sender| {
                sender.unbounded_send(change()).is_ok()
            });
        }

        fn notify_with<B, C, D, E>(&mut self, value: B, mut clone: C, change: D, mut notify: E)
            where C: FnMut(&B) -> B,
                  D: FnOnce(&mut Self, B),
                  E: FnMut(B) -> VecDiff<A> {

            let mut len = self.senders.len();

            if len == 0 {
                change(self, value);

            } else {
                let mut copy = Some(clone(&value));

                change(self, value);

                self.senders.retain(move |sender| {
                    let value = copy.take().unwrap();

                    len -= 1;

                    let value = if len == 0 {
                        value

                    } else {
                        let v = clone(&value);
                        copy = Some(value);
                        v
                    };

                    sender.unbounded_send(notify(value)).is_ok()
                });
            }
        }

        fn pop(&mut self) -> Option<A> {
            let value = self.values.pop();

            if value.is_some() {
                self.notify(|| VecDiff::Pop {});
            }

            value
        }

        fn remove(&mut self, index: usize) -> A {
            let len = self.values.len();

            let value = self.values.remove(index);

            if index == (len - 1) {
                self.notify(|| VecDiff::Pop {});

            } else {
                self.notify(|| VecDiff::RemoveAt { index });
            }

            value
        }

        fn move_from_to(&mut self, old_index: usize, new_index: usize) {
            let value = self.values.remove(old_index);
            self.values.insert(new_index, value);
            self.notify(|| VecDiff::Move { old_index, new_index });
        }

        fn clear(&mut self) {
            if self.values.len() > 0 {
                self.values.clear();

                self.notify(|| VecDiff::Clear {});
            }
        }

        fn retain<F>(&mut self, mut f: F) where F: FnMut(&A) -> bool {
            let mut len = self.values.len();

            if len > 0 {
                let mut index = 0;

                let mut removals = vec![];

                self.values.retain(|value| {
                    let output = f(value);

                    if !output {
                        removals.push(index);
                    }

                    index += 1;

                    output
                });

                if self.values.len() == 0 {
                    self.notify(|| VecDiff::Clear {});

                } else {
                    // TODO use VecDiff::Batch
                    for index in removals.into_iter().rev() {
                        len -= 1;

                        if index == len {
                            self.notify(|| VecDiff::Pop {});

                        } else {
                            self.notify(|| VecDiff::RemoveAt { index });
                        }
                    }
                }
            }
        }
    }

    impl<A: Copy> MutableVecState<A> {
        // This copies the Vec, but without calling clone
        // TODO better implementation of this ?
        // TODO prove that this doesn't call clone
        fn copy_values(values: &Vec<A>) -> Vec<A> {
            let mut output: Vec<A> = vec![];
            output.extend(values);
            output
        }

        fn signal_vec_copy(&mut self) -> MutableSignalVec<A> {
            let (sender, receiver) = mpsc::unbounded();

            if self.values.len() > 0 {
                sender.unbounded_send(VecDiff::Replace { values: Self::copy_values(&self.values) }).unwrap();
            }

            self.senders.push(sender);

            MutableSignalVec {
                receiver
            }
        }

        fn push_copy(&mut self, value: A) {
            self.values.push(value);
            self.notify(|| VecDiff::Push { value });
        }

        fn insert_copy(&mut self, index: usize, value: A) {
            if index == self.values.len() {
                self.push_copy(value);

            } else {
                self.values.insert(index, value);
                self.notify(|| VecDiff::InsertAt { index, value });
            }
        }

        fn set_copy(&mut self, index: usize, value: A) {
            self.values[index] = value;
            self.notify(|| VecDiff::UpdateAt { index, value });
        }

        fn replace_copy(&mut self, values: Vec<A>) {
            self.notify_with(values,
                Self::copy_values,
                |this, values| this.values = values,
                |values| VecDiff::Replace { values });
        }
    }

    impl<A: Clone> MutableVecState<A> {
        #[inline]
        fn notify_clone<B, C, D>(&mut self, value: B, change: C, notify: D)
            where B: Clone,
                  C: FnOnce(&mut Self, B),
                  D: FnMut(B) -> VecDiff<A> {

            self.notify_with(value, |a| a.clone(), change, notify)
        }

        // TODO change this to return a MutableSignalVecClone ?
        fn signal_vec_clone(&mut self) -> MutableSignalVec<A> {
            let (sender, receiver) = mpsc::unbounded();

            if self.values.len() > 0 {
                sender.unbounded_send(VecDiff::Replace { values: self.values.clone() }).unwrap();
            }

            self.senders.push(sender);

            MutableSignalVec {
                receiver
            }
        }

        fn push_clone(&mut self, value: A) {
            self.notify_clone(value,
                |this, value| this.values.push(value),
                |value| VecDiff::Push { value });
        }

        fn insert_clone(&mut self, index: usize, value: A) {
            if index == self.values.len() {
                self.push_clone(value);

            } else {
                self.notify_clone(value,
                    |this, value| this.values.insert(index, value),
                    |value| VecDiff::InsertAt { index, value });
            }
        }

        fn set_clone(&mut self, index: usize, value: A) {
            self.notify_clone(value,
                |this, value| this.values[index] = value,
                |value| VecDiff::UpdateAt { index, value });
        }

        fn replace_clone(&mut self, values: Vec<A>) {
            self.notify_clone(values,
                |this, values| this.values = values,
                |values| VecDiff::Replace { values });
        }
    }


    pub struct MutableVecLockSlice<'a, A> where A: 'a {
        lock: RwLockReadGuard<'a, MutableVecState<A>>,
    }

    impl<'a, A> Deref for MutableVecLockSlice<'a, A> {
        type Target = [A];

        #[inline]
        fn deref(&self) -> &Self::Target {
            &self.lock.values
        }
    }


    // TODO get rid of the Arc
    pub struct MutableVec<A>(Arc<RwLock<MutableVecState<A>>>);

    impl<A> MutableVec<A> {
        #[inline]
        pub fn new_with_values(values: Vec<A>) -> Self {
            MutableVec(Arc::new(RwLock::new(MutableVecState {
                values,
                senders: vec![],
            })))
        }

        #[inline]
        pub fn new() -> Self {
            Self::new_with_values(vec![])
        }

        #[inline]
        pub fn pop(&self) -> Option<A> {
            self.0.write().unwrap().pop()
        }

        #[inline]
        pub fn remove(&self, index: usize) -> A {
            self.0.write().unwrap().remove(index)
        }

        #[inline]
        pub fn clear(&self) {
            self.0.write().unwrap().clear()
        }

        #[inline]
        pub fn move_from_to(&self, old_index: usize, new_index: usize) {
            self.0.write().unwrap().move_from_to(old_index, new_index);
        }

        #[inline]
        pub fn retain<F>(&self, f: F) where F: FnMut(&A) -> bool {
            self.0.write().unwrap().retain(f)
        }

        // TODO return Result ?
        pub fn lock_slice(&self) -> MutableVecLockSlice<A> {
            MutableVecLockSlice {
                lock: self.0.read().unwrap(),
            }
        }

        #[inline]
        pub fn len(&self) -> usize {
            self.0.read().unwrap().values.len()
        }

        #[inline]
        pub fn is_empty(&self) -> bool {
            self.0.read().unwrap().values.is_empty()
        }
    }

    impl<A: Copy> MutableVec<A> {
        #[inline]
        pub fn signal_vec(&self) -> MutableSignalVec<A> {
            self.0.write().unwrap().signal_vec_copy()
        }

        #[inline]
        pub fn push(&self, value: A) {
            self.0.write().unwrap().push_copy(value)
        }

        #[inline]
        pub fn insert(&self, index: usize, value: A) {
            self.0.write().unwrap().insert_copy(index, value)
        }

        // TODO replace this with something else, like entry or IndexMut or whatever
        #[inline]
        pub fn set(&self, index: usize, value: A) {
            self.0.write().unwrap().set_copy(index, value)
        }

        #[inline]
        pub fn replace(&self, values: Vec<A>) {
            self.0.write().unwrap().replace_copy(values)
        }
    }

    impl<A: Clone> MutableVec<A> {
        #[inline]
        pub fn signal_vec_cloned(&self) -> MutableSignalVec<A> {
            self.0.write().unwrap().signal_vec_clone()
        }

        #[inline]
        pub fn push_cloned(&self, value: A) {
            self.0.write().unwrap().push_clone(value)
        }

        #[inline]
        pub fn insert_cloned(&self, index: usize, value: A) {
            self.0.write().unwrap().insert_clone(index, value)
        }

        // TODO replace this with something else, like entry or IndexMut or whatever
        #[inline]
        pub fn set_cloned(&self, index: usize, value: A) {
            self.0.write().unwrap().set_clone(index, value)
        }

        #[inline]
        pub fn replace_cloned(&self, values: Vec<A>) {
            self.0.write().unwrap().replace_clone(values)
        }
    }

    impl<T> Serialize for MutableVec<T> where T: Serialize {
        #[inline]
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
            self.0.read().unwrap().values.serialize(serializer)
        }
    }

    impl<'de, T> Deserialize<'de> for MutableVec<T> where T: Deserialize<'de> {
        #[inline]
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de> {
            <Vec<T>>::deserialize(deserializer).map(MutableVec::new_with_values)
        }
    }

    impl<T> Default for MutableVec<T> {
        #[inline]
        fn default() -> Self {
            MutableVec::new()
        }
    }


    pub struct MutableSignalVec<A> {
        receiver: mpsc::UnboundedReceiver<VecDiff<A>>,
    }

    impl<A> SignalVec for MutableSignalVec<A> {
        type Item = A;

        #[inline]
        fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecDiff<Self::Item>>> {
            self.receiver.poll_next(cx).unwrap()
        }
    }
}

pub use self::mutable_vec::*;


#[cfg(test)]
mod tests {
    use futures_core::{Future, Poll};
    use futures_executor::block_on;
    use super::*;
    use super::mutable_vec::MutableVec;


    struct Tester<A> {
        changes: Vec<Async<VecDiff<A>>>,
    }

    impl<A> Tester<A> {
        #[inline]
        fn new(changes: Vec<Async<VecDiff<A>>>) -> Self {
            Self { changes }
        }
    }

    impl<A> SignalVec for Tester<A> {
        type Item = A;

        #[inline]
        fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecDiff<Self::Item>>> {
            if self.changes.len() > 0 {
                match self.changes.remove(0) {
                    Async::Pending => {
                        cx.waker().wake();
                        Async::Pending
                    },
                    Async::Ready(change) => Async::Ready(Some(change)),
                }

            } else {
                Async::Ready(None)
            }
        }
    }


    struct TesterFuture<A, B> {
        signal: A,
        callback: B,
    }

    impl<A: SignalVec, B: FnMut(&mut A, VecDiff<A::Item>)> TesterFuture<A, B> {
        #[inline]
        fn new(signal: A, callback: B) -> Self {
            Self { signal, callback }
        }
    }

    impl<A, B> Future for TesterFuture<A, B>
        where A: SignalVec,
              B: FnMut(&mut A, VecDiff<A::Item>) {

        type Item = ();
        type Error = Never;

        #[inline]
        fn poll(&mut self, cx: &mut Context) -> Poll<Self::Item, Self::Error> {
            loop {
                return match self.signal.poll_vec_change(cx) {
                    Async::Ready(Some(change)) => {
                        (self.callback)(&mut self.signal, change);
                        continue;
                    },
                    Async::Ready(None) => Ok(Async::Ready(())),
                    Async::Pending => Ok(Async::Pending),
                }
            }
        }
    }

    fn run<A: SignalVec, B, C: FnMut(&mut A, VecDiff<A::Item>) -> B>(signal: A, mut callback: C) -> Vec<B> {
        let mut changes = vec![];

        block_on(TesterFuture::new(signal, |signal, change| {
            changes.push(callback(signal, change));
        })).unwrap();

        changes
    }


    #[test]
    fn send_sync() {
        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new());
        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new().signal_vec());
        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new().signal_vec_cloned());

        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new_with_values(vec![]));
        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new_with_values(vec![]).signal_vec());
        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new_with_values(vec![]).signal_vec_cloned());
    }

    #[test]
    fn filter() {
        #[derive(Debug, PartialEq, Eq)]
        struct Change {
            length: usize,
            indexes: Vec<bool>,
            change: VecDiff<u32>,
        }

        let input = Tester::new(vec![
            Async::Ready(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
            Async::Pending,
            Async::Ready(VecDiff::InsertAt { index: 0, value: 6 }),
            Async::Ready(VecDiff::InsertAt { index: 2, value: 7 }),
            Async::Pending,
            Async::Pending,
            Async::Pending,
            Async::Ready(VecDiff::InsertAt { index: 5, value: 8 }),
            Async::Ready(VecDiff::InsertAt { index: 7, value: 9 }),
            Async::Ready(VecDiff::InsertAt { index: 9, value: 10 }),
            Async::Pending,
            Async::Ready(VecDiff::InsertAt { index: 11, value: 11 }),
            Async::Pending,
            Async::Ready(VecDiff::InsertAt { index: 0, value: 0 }),
            Async::Pending,
            Async::Pending,
            Async::Ready(VecDiff::InsertAt { index: 1, value: 0 }),
            Async::Ready(VecDiff::InsertAt { index: 5, value: 0 }),
            Async::Pending,
            Async::Ready(VecDiff::InsertAt { index: 5, value: 12 }),
            Async::Pending,
            Async::Ready(VecDiff::RemoveAt { index: 0 }),
            Async::Ready(VecDiff::RemoveAt { index: 0 }),
            Async::Pending,
            Async::Ready(VecDiff::RemoveAt { index: 0 }),
            Async::Ready(VecDiff::RemoveAt { index: 1 }),
            Async::Pending,
            Async::Ready(VecDiff::RemoveAt { index: 0 }),
            Async::Pending,
            Async::Ready(VecDiff::RemoveAt { index: 0 }),
        ]);

        let output = input.filter(|&x| x == 3 || x == 4 || x > 5);

        assert_eq!(Filter::len(&output), 0);
        assert_eq!(output.indexes, vec![]);

        let changes = run(output, |output, change| {
            Change {
                change: change,
                length: Filter::len(&output),
                indexes: output.indexes.clone(),
            }
        });

        assert_eq!(changes, vec![
            Change { length: 2, indexes: vec![false, false, false, true, true, false], change: VecDiff::Replace { values: vec![3, 4] } },
            Change { length: 3, indexes: vec![true, false, false, false, true, true, false], change: VecDiff::InsertAt { index: 0, value: 6 } },
            Change { length: 4, indexes: vec![true, false, true, false, false, true, true, false], change: VecDiff::InsertAt { index: 1, value: 7 } },
            Change { length: 5, indexes: vec![true, false, true, false, false, true, true, true, false], change: VecDiff::InsertAt { index: 2, value: 8 } },
            Change { length: 6, indexes: vec![true, false, true, false, false, true, true, true, true, false], change: VecDiff::InsertAt { index: 4, value: 9 } },
            Change { length: 7, indexes: vec![true, false, true, false, false, true, true, true, true, true, false], change: VecDiff::InsertAt { index: 6, value: 10 } },
            Change { length: 8, indexes: vec![true, false, true, false, false, true, true, true, true, true, false, true], change: VecDiff::InsertAt { index: 7, value: 11 } },
            Change { length: 9, indexes: vec![false, false, true, false, true, true, false, false, false, true, true, true, true, true, false, true], change: VecDiff::InsertAt { index: 2, value: 12 } },
            Change { length: 8, indexes: vec![false, true, true, false, false, false, true, true, true, true, true, false, true], change: VecDiff::RemoveAt { index: 0 } },
            Change { length: 7, indexes: vec![false, true, false, false, false, true, true, true, true, true, false, true], change: VecDiff::RemoveAt { index: 0 } },
            Change { length: 6, indexes: vec![false, false, false, true, true, true, true, true, false, true], change: VecDiff::RemoveAt { index: 0 } },
        ]);
    }
}
