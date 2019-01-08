use std::iter::Sum;
use std::collections::VecDeque;
use std::pin::Pin;
use std::marker::Unpin;
use std::cmp::Ordering;
use futures_core::task::LocalWaker;
use futures_core::{Future, Stream, Poll};
use futures_util::stream;
use futures_util::stream::StreamExt;
use signal::{Signal, Mutable, ReadOnlyMutable};


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


// TODO impl for AssertUnwindSafe ?
pub trait SignalVec {
    type Item;

    fn poll_vec_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>>;
}


// Copied from Future in the Rust stdlib
impl<'a, A> SignalVec for &'a mut A where A: ?Sized + SignalVec + Unpin {
    type Item = A::Item;

    #[inline]
    fn poll_vec_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        A::poll_vec_change(Pin::new(&mut **self), waker)
    }
}

// Copied from Future in the Rust stdlib
impl<A> SignalVec for Box<A> where A: ?Sized + SignalVec + Unpin {
    type Item = A::Item;

    #[inline]
    fn poll_vec_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        A::poll_vec_change(Pin::new(&mut *self), waker)
    }
}

// Copied from Future in the Rust stdlib
impl<A> SignalVec for Pin<A>
    where A: Unpin + ::std::ops::DerefMut,
          A::Target: SignalVec {
    type Item = <<A as ::std::ops::Deref>::Target as SignalVec>::Item;

    #[inline]
    fn poll_vec_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        Pin::get_mut(self).as_mut().poll_vec_change(waker)
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
            pending: VecDeque::new(),
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

    #[inline]
    fn filter_signal_cloned<A, F>(self, callback: F) -> FilterSignalCloned<Self, A, F>
        where A: Signal<Item = bool>,
              F: FnMut(&Self::Item) -> A,
              Self: Sized {
        FilterSignalCloned {
            signal: Some(self),
            signals: vec![],
            pending: VecDeque::new(),
            callback,
        }
    }

    #[inline]
    fn sum(self) -> SumSignal<Self>
        where Self::Item: for<'a> Sum<&'a Self::Item>,
              Self: Sized {
        SumSignal {
            signal: Some(self),
            first: true,
            values: vec![],
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
    fn to_stream(self) -> SignalVecStream<Self> where Self: Sized {
        SignalVecStream {
            signal: self,
        }
    }

    #[inline]
    // TODO file Rust bug about bad error message when `callback` isn't marked as `mut`
    fn for_each<U, F>(self, callback: F) -> ForEach<Self, U, F>
        where U: Future<Output = ()>,
              F: FnMut(VecDiff<Self::Item>) -> U,
              Self: Sized {
        // TODO a little hacky
        ForEach {
            inner: SignalVecStream {
                signal: self,
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
        where A: Future<Output = ()>,
              F: FnMut(&Self::Item) -> A,
              Self: Sized {
        DelayRemove {
            signal: Some(self),
            futures: vec![],
            pending: VecDeque::new(),
            callback: f,
        }
    }

    /// A convenience for calling `SignalVec::poll_vec_change` on `Unpin` types.
    #[inline]
    fn poll_vec_change_unpin(&mut self, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> where Self: Unpin + Sized {
        Pin::new(self).poll_vec_change(waker)
    }
}

// TODO why is this ?Sized
impl<T: ?Sized> SignalVecExt for T where T: SignalVec {}


#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct Always<A> {
    values: Option<Vec<A>>,
}

impl<A> Unpin for Always<A> {}

impl<A> SignalVec for Always<A> {
    type Item = A;

    fn poll_vec_change(mut self: Pin<&mut Self>, _waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        Poll::Ready(self.values.take().map(|values| VecDiff::Replace { values }))
    }
}

#[inline]
pub fn always<A>(values: Vec<A>) -> Always<A> {
    Always {
        values: Some(values),
    }
}


#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct ForEach<A, B, C> {
    inner: stream::ForEach<SignalVecStream<A>, B, C>,
}

impl<A, B, C> Unpin for ForEach<A, B, C> where A: Unpin, B: Unpin {}

impl<A, B, C> Future for ForEach<A, B, C>
    where A: SignalVec,
          B: Future<Output = ()>,
          C: FnMut(VecDiff<A::Item>) -> B {
    type Output = ();

    #[inline]
    fn poll(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Self::Output> {
        unsafe_project!(self => {
            pin inner,
        });

        inner.poll(waker)
    }
}


#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct Map<A, B> {
    signal: A,
    callback: B,
}

impl<A, B> Unpin for Map<A, B> where A: Unpin {}

impl<A, B, F> SignalVec for Map<A, F>
    where A: SignalVec,
          F: FnMut(A::Item) -> B {
    type Item = B;

    // TODO should this inline ?
    #[inline]
    fn poll_vec_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        unsafe_project!(self => {
            pin signal,
            mut callback,
        });

        signal.poll_vec_change(waker).map(|some| some.map(|change| change.map(|value| callback(value))))
    }
}


#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct Enumerate<A> {
    signal: A,
    mutables: Vec<Mutable<Option<usize>>>,
}

impl<A> Unpin for Enumerate<A> where A: Unpin {}

impl<A> SignalVec for Enumerate<A> where A: SignalVec {
    type Item = (ReadOnlyMutable<Option<usize>>, A::Item);

    #[inline]
    fn poll_vec_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        fn increment_indexes(range: &[Mutable<Option<usize>>]) {
            for mutable in range {
                mutable.replace_with(|value| value.map(|value| value + 1));
            }
        }

        fn decrement_indexes(range: &[Mutable<Option<usize>>]) {
            for mutable in range {
                mutable.replace_with(|value| value.map(|value| value - 1));
            }
        }

        unsafe_project!(self => {
            pin signal,
            mut mutables,
        });

        // TODO use map ?
        match signal.poll_vec_change(waker) {
            Poll::Ready(Some(change)) => Poll::Ready(Some(match change {
                VecDiff::Replace { values } => {
                    for mutable in mutables.drain(..) {
                        // TODO use set_neq ?
                        mutable.set(None);
                    }

                    *mutables = Vec::with_capacity(values.len());

                    VecDiff::Replace {
                        values: values.into_iter().enumerate().map(|(index, value)| {
                            let mutable = Mutable::new(Some(index));
                            let read_only = mutable.read_only();
                            mutables.push(mutable);
                            (read_only, value)
                        }).collect()
                    }
                },

                VecDiff::InsertAt { index, value } => {
                    let mutable = Mutable::new(Some(index));
                    let read_only = mutable.read_only();

                    mutables.insert(index, mutable);

                    increment_indexes(&mutables[(index + 1)..]);

                    VecDiff::InsertAt { index, value: (read_only, value) }
                },

                VecDiff::UpdateAt { index, value } => {
                    VecDiff::UpdateAt { index, value: (mutables[index].read_only(), value) }
                },

                VecDiff::Push { value } => {
                    let mutable = Mutable::new(Some(mutables.len()));
                    let read_only = mutable.read_only();

                    mutables.push(mutable);

                    VecDiff::Push { value: (read_only, value) }
                },

                VecDiff::Move { old_index, new_index } => {
                    let mutable = mutables.remove(old_index);

                    // TODO figure out a way to avoid this clone ?
                    mutables.insert(new_index, mutable.clone());

                    // TODO test this
                    if old_index < new_index {
                        decrement_indexes(&mutables[old_index..new_index]);

                    } else if new_index < old_index {
                        increment_indexes(&mutables[(new_index + 1)..(old_index + 1)]);
                    }

                    // TODO use set_neq ?
                    mutable.set(Some(new_index));

                    VecDiff::Move { old_index, new_index }
                },

                VecDiff::RemoveAt { index } => {
                    let mutable = mutables.remove(index);

                    decrement_indexes(&mutables[index..]);

                    // TODO use set_neq ?
                    mutable.set(None);

                    VecDiff::RemoveAt { index }
                },

                VecDiff::Pop {} => {
                    let mutable = mutables.pop().unwrap();

                    // TODO use set_neq ?
                    mutable.set(None);

                    VecDiff::Pop {}
                },

                VecDiff::Clear {} => {
                    for mutable in mutables.drain(..) {
                        // TODO use set_neq ?
                        mutable.set(None);
                    }

                    VecDiff::Clear {}
                },
            })),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}


// This is an optimization to allow a SignalVec to efficiently "return" multiple VecDiff
// TODO can this be made more efficient ?
struct PendingBuilder<A> {
    first: Option<A>,
    rest: VecDeque<A>,
}

impl<A> PendingBuilder<A> {
    fn new() -> Self {
        Self {
            first: None,
            rest: VecDeque::new(),
        }
    }

    fn push(&mut self, value: A) {
        if let None = self.first {
            self.first = Some(value);

        } else {
            self.rest.push_back(value);
        }
    }
}


fn unwrap<A>(x: Poll<Option<A>>) -> A {
    match x {
        Poll::Ready(Some(x)) => x,
        _ => panic!("Signal did not return a value"),
    }
}

#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct MapSignal<A, B, F> where B: Signal {
    signal: Option<A>,
    // TODO is there a more efficient way to implement this ?
    signals: Vec<Option<Pin<Box<B>>>>,
    pending: VecDeque<VecDiff<B::Item>>,
    callback: F,
}

impl<A, B, F> Unpin for MapSignal<A, B, F> where A: Unpin, B: Signal {}

impl<A, B, F> SignalVec for MapSignal<A, B, F>
    where A: SignalVec,
          B: Signal,
          F: FnMut(A::Item) -> B {
    type Item = B::Item;

    fn poll_vec_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        unsafe_project!(self => {
            pin signal,
            mut signals,
            mut pending,
            mut callback,
        });

        if let Some(diff) = pending.pop_front() {
            return Poll::Ready(Some(diff));
        }

        let mut new_pending = PendingBuilder::new();

        let done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(waker)) {
                None => {
                    true
                },
                Some(Poll::Ready(None)) => {
                    Pin::set(signal, None);
                    true
                },
                Some(Poll::Ready(Some(change))) => {
                    new_pending.push(match change {
                        VecDiff::Replace { values } => {
                            *signals = Vec::with_capacity(values.len());

                            VecDiff::Replace {
                                values: values.into_iter().map(|value| {
                                    let mut signal = Box::pin(callback(value));
                                    let poll = unwrap(signal.as_mut().poll_change(waker));
                                    signals.push(Some(signal));
                                    poll
                                }).collect()
                            }
                        },

                        VecDiff::InsertAt { index, value } => {
                            let mut signal = Box::pin(callback(value));
                            let poll = unwrap(signal.as_mut().poll_change(waker));
                            signals.insert(index, Some(signal));
                            VecDiff::InsertAt { index, value: poll }
                        },

                        VecDiff::UpdateAt { index, value } => {
                            let mut signal = Box::pin(callback(value));
                            let poll = unwrap(signal.as_mut().poll_change(waker));
                            signals[index] = Some(signal);
                            VecDiff::UpdateAt { index, value: poll }
                        },

                        VecDiff::Push { value } => {
                            let mut signal = Box::pin(callback(value));
                            let poll = unwrap(signal.as_mut().poll_change(waker));
                            signals.push(Some(signal));
                            VecDiff::Push { value: poll }
                        },

                        VecDiff::Move { old_index, new_index } => {
                            let value = signals.remove(old_index);
                            signals.insert(new_index, value);
                            VecDiff::Move { old_index, new_index }
                        },

                        VecDiff::RemoveAt { index } => {
                            signals.remove(index);
                            VecDiff::RemoveAt { index }
                        },

                        VecDiff::Pop {} => {
                            signals.pop().unwrap();
                            VecDiff::Pop {}
                        },

                        VecDiff::Clear {} => {
                            signals.clear();
                            VecDiff::Clear {}
                        },
                    });

                    continue;
                },
                Some(Poll::Pending) => false,
            };
        };

        let mut has_pending = false;

        // TODO ensure that this is as efficient as possible
        // TODO make this more efficient (e.g. using a similar strategy as FuturesUnordered)
        for (index, signal) in signals.as_mut_slice().into_iter().enumerate() {
            // TODO use a loop until the value stops changing ?
            match signal.as_mut().map(|s| s.as_mut().poll_change(waker)) {
                Some(Poll::Ready(Some(value))) => {
                    new_pending.push(VecDiff::UpdateAt { index, value });
                },
                Some(Poll::Ready(None)) => {
                    *signal = None;
                },
                Some(Poll::Pending) => {
                    has_pending = true;
                },
                None => {},
            }
        }

        if let Some(first) = new_pending.first {
            *pending = new_pending.rest;
            Poll::Ready(Some(first))

        } else if done && !has_pending {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}


#[derive(Debug)]
struct FilterSignalClonedState<A, B> {
    // TODO is there a more efficient way to implement this ?
    signal: Option<Pin<Box<B>>>,
    value: A,
    exists: bool,
}

#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct FilterSignalCloned<A, B, F> where A: SignalVec {
    signal: Option<A>,
    signals: Vec<FilterSignalClonedState<A::Item, B>>,
    pending: VecDeque<VecDiff<A::Item>>,
    callback: F,
}

impl<A, B, F> FilterSignalCloned<A, B, F> where A: SignalVec {
    fn find_index(signals: &[FilterSignalClonedState<A::Item, B>], index: usize) -> usize {
        signals[0..index].into_iter().filter(|x| x.exists).count()
    }
}

impl<A, B, F> Unpin for FilterSignalCloned<A, B, F> where A: Unpin + SignalVec {}

impl<A, B, F> SignalVec for FilterSignalCloned<A, B, F>
    where A: SignalVec,
          A::Item: Clone,
          B: Signal<Item = bool>,
          F: FnMut(&A::Item) -> B {
    type Item = A::Item;

    fn poll_vec_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        unsafe_project!(self => {
            pin signal,
            mut signals,
            mut pending,
            mut callback,
        });

        if let Some(diff) = pending.pop_front() {
            return Poll::Ready(Some(diff));
        }

        let mut new_pending = PendingBuilder::new();

        // TODO maybe it should check the filter signals first, before checking the signalvec ?
        let done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(waker)) {
                None => true,
                Some(Poll::Ready(None)) => {
                    Pin::set(signal, None);
                    true
                },
                Some(Poll::Ready(Some(change))) => {
                    new_pending.push(match change {
                        VecDiff::Replace { values } => {
                            *signals = Vec::with_capacity(values.len());

                            VecDiff::Replace {
                                values: values.into_iter().filter(|value| {
                                    let mut signal = Box::pin(callback(value));
                                    let poll = unwrap(signal.as_mut().poll_change(waker));

                                    signals.push(FilterSignalClonedState {
                                        signal: Some(signal),
                                        value: value.clone(),
                                        exists: poll,
                                    });

                                    poll
                                }).collect()
                            }
                        },

                        VecDiff::InsertAt { index, value } => {
                            let mut signal = Box::pin(callback(&value));
                            let poll = unwrap(signal.as_mut().poll_change(waker));

                            signals.insert(index, FilterSignalClonedState {
                                signal: Some(signal),
                                value: value.clone(),
                                exists: poll,
                            });

                            if poll {
                                VecDiff::InsertAt { index: Self::find_index(signals, index), value }

                            } else {
                                continue;
                            }
                        },

                        VecDiff::UpdateAt { index, value } => {
                            let mut signal = Box::pin(callback(&value));
                            let new_poll = unwrap(signal.as_mut().poll_change(waker));

                            let old_poll = {
                                let state = &mut signals[index];

                                let exists = state.exists;

                                state.signal = Some(signal);
                                state.value = value.clone();
                                state.exists = new_poll;

                                exists
                            };

                            if new_poll {
                                if old_poll {
                                    VecDiff::UpdateAt { index: Self::find_index(signals, index), value }

                                } else {
                                    VecDiff::InsertAt { index: Self::find_index(signals, index), value }
                                }

                            } else {
                                if old_poll {
                                    VecDiff::RemoveAt { index: Self::find_index(signals, index) }

                                } else {
                                    continue;
                                }
                            }
                        },

                        VecDiff::Push { value } => {
                            let mut signal = Box::pin(callback(&value));
                            let poll = unwrap(signal.as_mut().poll_change(waker));

                            signals.push(FilterSignalClonedState {
                                signal: Some(signal),
                                value: value.clone(),
                                exists: poll,
                            });

                            if poll {
                                VecDiff::Push { value }

                            } else {
                                continue;
                            }
                        },

                        // TODO unit tests for this
                        VecDiff::Move { old_index, new_index } => {
                            let state = signals.remove(old_index);
                            let exists = state.exists;

                            signals.insert(new_index, state);

                            if exists {
                                VecDiff::Move {
                                    old_index: Self::find_index(signals, old_index),
                                    new_index: Self::find_index(signals, new_index),
                                }

                            } else {
                                continue;
                            }
                        },

                        VecDiff::RemoveAt { index } => {
                            let state = signals.remove(index);

                            if state.exists {
                                VecDiff::RemoveAt { index: Self::find_index(signals, index) }

                            } else {
                                continue;
                            }
                        },

                        VecDiff::Pop {} => {
                            let state = signals.pop().expect("Cannot pop from empty vec");

                            if state.exists {
                                VecDiff::Pop {}

                            } else {
                                continue;
                            }
                        },

                        VecDiff::Clear {} => {
                            signals.clear();
                            VecDiff::Clear {}
                        },
                    });

                    continue;
                },
                Some(Poll::Pending) => false,
            }
        };

        let mut has_pending = false;

        let mut real_index = 0;

        // TODO ensure that this is as efficient as possible
        // TODO make this more efficient (e.g. using a similar strategy as FuturesUnordered)
        // TODO replace this with find_map ?
        // TODO use rev so that way it can use VecDiff::Pop ?
        for state in signals.as_mut_slice().into_iter() {
            let old = state.exists;

            // TODO is this loop a good idea ?
            loop {
                match state.signal.as_mut().map(|s| s.as_mut().poll_change(waker)) {
                    Some(Poll::Ready(Some(exists))) => {
                        state.exists = exists;
                        continue;
                    },
                    Some(Poll::Ready(None)) => {
                        state.signal = None;
                    },
                    Some(Poll::Pending) => {
                        has_pending = true;
                    },
                    None => {},
                }
                break;
            }

            if state.exists != old {
                // TODO test these
                // TODO use Push and Pop when the index is at the end
                if state.exists {
                    new_pending.push(VecDiff::InsertAt { index: real_index, value: state.value.clone() });

                } else {
                    new_pending.push(VecDiff::RemoveAt { index: real_index });
                }
            }

            if state.exists {
                real_index += 1;
            }
        }

        if let Some(first) = new_pending.first {
            *pending = new_pending.rest;
            Poll::Ready(Some(first))

        } else if done && !has_pending {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Len<A> {
    signal: Option<A>,
    first: bool,
    len: usize,
}

impl<A> Unpin for Len<A> where A: Unpin {}

impl<A> Signal for Len<A> where A: SignalVec {
    type Item = usize;

    fn poll_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin signal,
            mut first,
            mut len,
        });

        let mut changed = false;

        let done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(waker)) {
                None => {
                    true
                },
                Some(Poll::Ready(None)) => {
                    Pin::set(signal, None);
                    true
                },
                Some(Poll::Ready(Some(change))) => {
                    match change {
                        VecDiff::Replace { values } => {
                            let new_len = values.len();

                            if *len != new_len {
                                *len = new_len;
                                changed = true;
                            }
                        },

                        VecDiff::InsertAt { .. } | VecDiff::Push { .. } => {
                            *len += 1;
                            changed = true;
                        },

                        VecDiff::UpdateAt { .. } | VecDiff::Move { .. } => {},

                        VecDiff::RemoveAt { .. } | VecDiff::Pop {} => {
                            *len -= 1;
                            changed = true;
                        },

                        VecDiff::Clear {} => {
                            if *len != 0 {
                                *len = 0;
                                changed = true;
                            }
                        },
                    }

                    continue;
                },
                Some(Poll::Pending) => {
                    false
                },
            };
        };

        if changed || *first {
            *first = false;
            // TODO is this correct ?
            Poll::Ready(Some(*len))

        } else if done {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}


#[derive(Debug)]
#[must_use = "Streams do nothing unless polled"]
pub struct SignalVecStream<A> {
    signal: A,
}

impl<A> Unpin for SignalVecStream<A> where A: Unpin {}

impl<A: SignalVec> Stream for SignalVecStream<A> {
    type Item = VecDiff<A::Item>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin signal,
        });

        signal.poll_vec_change(waker)
    }
}


#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct Filter<A, B> {
    // TODO use a bit vec for smaller size
    indexes: Vec<bool>,
    signal: A,
    callback: B,
}

impl<A, B> Filter<A, B> {
    fn find_index(indexes: &[bool], index: usize) -> usize {
        indexes[0..index].into_iter().filter(|x| **x).count()
    }
}

impl<A, B> Unpin for Filter<A, B> where A: Unpin {}

impl<A, F> SignalVec for Filter<A, F>
    where A: SignalVec,
          F: FnMut(&A::Item) -> bool {
    type Item = A::Item;

    fn poll_vec_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        unsafe_project!(self => {
            mut indexes,
            pin signal,
            mut callback,
        });

        loop {
            return match signal.as_mut().poll_vec_change(waker) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Ready(Some(change)) => match change {
                    VecDiff::Replace { values } => {
                        *indexes = Vec::with_capacity(values.len());

                        Poll::Ready(Some(VecDiff::Replace {
                            values: values.into_iter().filter(|value| {
                                let keep = callback(value);
                                indexes.push(keep);
                                keep
                            }).collect()
                        }))
                    },

                    VecDiff::InsertAt { index, value } => {
                        if callback(&value) {
                            indexes.insert(index, true);
                            Poll::Ready(Some(VecDiff::InsertAt { index: Self::find_index(indexes, index), value }))

                        } else {
                            indexes.insert(index, false);
                            continue;
                        }
                    },

                    VecDiff::UpdateAt { index, value } => {
                        if callback(&value) {
                            if indexes[index] {
                                Poll::Ready(Some(VecDiff::UpdateAt { index: Self::find_index(indexes, index), value }))

                            } else {
                                indexes[index] = true;
                                Poll::Ready(Some(VecDiff::InsertAt { index: Self::find_index(indexes, index), value }))
                            }

                        } else {
                            if indexes[index] {
                                indexes[index] = false;
                                Poll::Ready(Some(VecDiff::RemoveAt { index: Self::find_index(indexes, index) }))

                            } else {
                                continue;
                            }
                        }
                    },

                    // TODO unit tests for this
                    VecDiff::Move { old_index, new_index } => {
                        if indexes.remove(old_index) {
                            indexes.insert(new_index, true);

                            Poll::Ready(Some(VecDiff::Move {
                                old_index: Self::find_index(indexes, old_index),
                                new_index: Self::find_index(indexes, new_index),
                            }))

                        } else {
                            indexes.insert(new_index, false);
                            continue;
                        }
                    },

                    VecDiff::RemoveAt { index } => {
                        if indexes.remove(index) {
                            Poll::Ready(Some(VecDiff::RemoveAt { index: Self::find_index(indexes, index) }))

                        } else {
                            continue;
                        }
                    },

                    VecDiff::Push { value } => {
                        if callback(&value) {
                            indexes.push(true);
                            Poll::Ready(Some(VecDiff::Push { value }))

                        } else {
                            indexes.push(false);
                            continue;
                        }
                    },

                    VecDiff::Pop {} => {
                        if indexes.pop().expect("Cannot pop from empty vec") {
                            Poll::Ready(Some(VecDiff::Pop {}))

                        } else {
                            continue;
                        }
                    },

                    VecDiff::Clear {} => {
                        indexes.clear();
                        Poll::Ready(Some(VecDiff::Clear {}))
                    },
                },
            }
        }
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct SumSignal<A> where A: SignalVec {
    signal: Option<A>,
    first: bool,
    values: Vec<A::Item>,
}

impl<A> Unpin for SumSignal<A> where A: Unpin + SignalVec {}

impl<A> Signal for SumSignal<A>
    where A: SignalVec,
          A::Item: for<'a> Sum<&'a A::Item> {
    type Item = A::Item;

    fn poll_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin signal,
            mut first,
            mut values,
        });

        let mut changed = false;

        let done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(waker)) {
                None => {
                    true
                },
                Some(Poll::Ready(None)) => {
                    Pin::set(signal, None);
                    true
                },
                Some(Poll::Ready(Some(change))) => {
                    match change {
                        VecDiff::Replace { values: new_values } => {
                            *values = new_values;
                        },

                        VecDiff::InsertAt { index, value } => {
                            values.insert(index, value);
                        },

                        VecDiff::Push { value } => {
                            values.push(value);
                        },

                        VecDiff::UpdateAt { index, value } => {
                            values[index] = value;
                        },

                        VecDiff::Move { old_index, new_index } => {
                            let value = values.remove(old_index);
                            values.insert(new_index, value);
                            // Moving shouldn't recalculate the sum
                            continue;
                        },

                        VecDiff::RemoveAt { index } => {
                            values.remove(index);
                        },

                        VecDiff::Pop {} => {
                            values.pop().unwrap();
                        },

                        VecDiff::Clear {} => {
                            values.clear();
                        },
                    }

                    changed = true;
                    continue;
                },
                Some(Poll::Pending) => {
                    false
                },
            };
        };

        if changed || *first {
            *first = false;

            Poll::Ready(Some(Sum::sum(values.iter())))

        } else if done {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}


#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct SortByCloned<A, B> where A: SignalVec {
    pending: Option<VecDiff<A::Item>>,
    values: Vec<A::Item>,
    indexes: Vec<usize>,
    signal: A,
    compare: B,
}

impl<A, F> SortByCloned<A, F>
    where A: SignalVec,
          F: FnMut(&A::Item, &A::Item) -> Ordering {

    // TODO should this inline ?
    fn binary_search(values: &[A::Item], indexes: &[usize], compare: &mut F, index: usize) -> Result<usize, usize> {
        let value = &values[index];

        // TODO use get_unchecked ?
        indexes.binary_search_by(|i| compare(&values[*i], value).then_with(|| i.cmp(&index)))
    }

    fn binary_search_insert(values: &[A::Item], indexes: &[usize], compare: &mut F, index: usize) -> usize {
        match Self::binary_search(values, indexes, compare, index) {
            Ok(_) => panic!("Value already exists"),
            Err(new_index) => new_index,
        }
    }

    fn binary_search_remove(values: &[A::Item], indexes: &[usize], compare: &mut F, index: usize) -> usize {
        Self::binary_search(values, indexes, compare, index).expect("Could not find value")
    }

    fn increment_indexes(indexes: &mut Vec<usize>, start: usize) {
        for index in indexes {
            let i = *index;

            if i >= start {
                *index = i + 1;
            }
        }
    }

    fn decrement_indexes(indexes: &mut Vec<usize>, start: usize) {
        for index in indexes {
            let i = *index;

            if i > start {
                *index = i - 1;
            }
        }
    }

    fn insert_at(indexes: &mut Vec<usize>, sorted_index: usize, index: usize, value: A::Item) -> VecDiff<A::Item> {
        if sorted_index == indexes.len() {
            indexes.push(index);

            VecDiff::Push {
                value,
            }

        } else {
            indexes.insert(sorted_index, index);

            VecDiff::InsertAt {
                index: sorted_index,
                value,
            }
        }
    }

    fn remove_at(indexes: &mut Vec<usize>, sorted_index: usize) -> Poll<Option<VecDiff<A::Item>>> {
        if sorted_index == (indexes.len() - 1) {
            indexes.pop();

            Poll::Ready(Some(VecDiff::Pop {}))

        } else {
            indexes.remove(sorted_index);

            Poll::Ready(Some(VecDiff::RemoveAt {
                index: sorted_index,
            }))
        }
    }
}

impl<A, B> Unpin for SortByCloned<A, B> where A: Unpin + SignalVec {}

// TODO implementation of this for Copy
impl<A, F> SignalVec for SortByCloned<A, F>
    where A: SignalVec,
          F: FnMut(&A::Item, &A::Item) -> Ordering,
          A::Item: Clone {
    type Item = A::Item;

    // TODO figure out a faster implementation of this
    fn poll_vec_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        unsafe_project!(self => {
            mut pending,
            mut values,
            mut indexes,
            pin signal,
            mut compare,
        });

        match pending.take() {
            Some(value) => Poll::Ready(Some(value)),
            None => loop {
                return match signal.as_mut().poll_vec_change(waker) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Ready(Some(change)) => match change {
                        VecDiff::Replace { values: mut new_values } => {
                            // TODO can this be made faster ?
                            let mut new_indexes: Vec<usize> = (0..new_values.len()).collect();

                            // TODO use get_unchecked ?
                            new_indexes.sort_unstable_by(|a, b| compare(&new_values[*a], &new_values[*b]).then_with(|| a.cmp(b)));

                            let output = Poll::Ready(Some(VecDiff::Replace {
                                // TODO use get_unchecked ?
                                values: new_indexes.iter().map(|i| new_values[*i].clone()).collect()
                            }));

                            *values = new_values;
                            *indexes = new_indexes;

                            output
                        },

                        VecDiff::InsertAt { index, value } => {
                            let new_value = value.clone();

                            values.insert(index, value);

                            Self::increment_indexes(indexes, index);

                            let sorted_index = Self::binary_search_insert(values, indexes, compare, index);

                            Poll::Ready(Some(Self::insert_at(indexes, sorted_index, index, new_value)))
                        },

                        VecDiff::Push { value } => {
                            let new_value = value.clone();

                            let index = values.len();

                            values.push(value);

                            let sorted_index = Self::binary_search_insert(values, indexes, compare, index);

                            Poll::Ready(Some(Self::insert_at(indexes, sorted_index, index, new_value)))
                        },

                        VecDiff::UpdateAt { index, value } => {
                            let old_index = Self::binary_search_remove(values, indexes, compare, index);

                            let old_output = Self::remove_at(indexes, old_index);

                            let new_value = value.clone();

                            values[index] = value;

                            let new_index = Self::binary_search_insert(values, indexes, compare, index);

                            if old_index == new_index {
                                indexes.insert(new_index, index);

                                Poll::Ready(Some(VecDiff::UpdateAt {
                                    index: new_index,
                                    value: new_value,
                                }))

                            } else {
                                let new_output = Self::insert_at(indexes, new_index, index, new_value);
                                *pending = Some(new_output);

                                old_output
                            }
                        },

                        VecDiff::RemoveAt { index } => {
                            let sorted_index = Self::binary_search_remove(values, indexes, compare, index);

                            values.remove(index);

                            Self::decrement_indexes(indexes, index);

                            Self::remove_at(indexes, sorted_index)
                        },

                        // TODO can this be made more efficient ?
                        VecDiff::Move { old_index, new_index } => {
                            let old_sorted_index = Self::binary_search_remove(values, indexes, compare, old_index);

                            let value = values.remove(old_index);

                            Self::decrement_indexes(indexes, old_index);

                            indexes.remove(old_sorted_index);

                            values.insert(new_index, value);

                            Self::increment_indexes(indexes, new_index);

                            let new_sorted_index = Self::binary_search_insert(values, indexes, compare, new_index);

                            indexes.insert(new_sorted_index, new_index);

                            if old_sorted_index == new_sorted_index {
                                continue;

                            } else {
                                Poll::Ready(Some(VecDiff::Move {
                                    old_index: old_sorted_index,
                                    new_index: new_sorted_index,
                                }))
                            }
                        },

                        VecDiff::Pop {} => {
                            let index = values.len() - 1;

                            let sorted_index = Self::binary_search_remove(values, indexes, compare, index);

                            values.pop();

                            Self::remove_at(indexes, sorted_index)
                        },

                        VecDiff::Clear {} => {
                            values.clear();
                            indexes.clear();
                            Poll::Ready(Some(VecDiff::Clear {}))
                        },
                    },
                }
            },
        }
    }
}


#[derive(Debug)]
struct DelayRemoveState<A> {
    // TODO is it possible to implement this more efficiently ?
    future: Pin<Box<A>>,
    is_removing: bool,
}

impl<A> DelayRemoveState<A> {
    #[inline]
    fn new(future: A) -> Self {
        Self {
            future: Box::pin(future),
            is_removing: false,
        }
    }
}

#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct DelayRemove<A, B, F> where A: SignalVec {
    signal: Option<A>,
    futures: Vec<DelayRemoveState<B>>,
    pending: VecDeque<VecDiff<A::Item>>,
    callback: F,
}

impl<S, A, F> DelayRemove<S, A, F>
    where S: SignalVec,
          A: Future<Output = ()>,
          F: FnMut(&S::Item) -> A {

    fn remove_index(futures: &mut Vec<DelayRemoveState<A>>, index: usize) -> VecDiff<S::Item> {
        if index == (futures.len() - 1) {
            futures.pop();
            VecDiff::Pop {}

        } else {
            futures.remove(index);
            VecDiff::RemoveAt { index }
        }
    }

    fn should_remove(state: &mut DelayRemoveState<A>, waker: &LocalWaker) -> bool {
        assert!(!state.is_removing);

        if state.future.as_mut().poll(waker).is_ready() {
            true

        } else {
            state.is_removing = true;
            false
        }
    }

    fn find_index(futures: &[DelayRemoveState<A>], parent_index: usize) -> Option<usize> {
        let mut seen = 0;

        // TODO is there a combinator that can simplify this ?
        futures.into_iter().position(|state| {
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

    fn find_last_index(futures: &[DelayRemoveState<A>]) -> Option<usize> {
        futures.into_iter().rposition(|state| !state.is_removing)
    }

    fn remove_existing_futures(futures: &mut Vec<DelayRemoveState<A>>, pending: &mut PendingBuilder<VecDiff<S::Item>>, waker: &LocalWaker) {
        let mut indexes = vec![];

        for (index, future) in futures.iter_mut().enumerate() {
            if !future.is_removing {
                if Self::should_remove(future, waker) {
                    indexes.push(index);
                }
            }
        }

        // This uses rev so that way it will return VecDiff::Pop in more situations
        for index in indexes.into_iter().rev() {
            pending.push(Self::remove_index(futures, index));
        }
    }
}

impl<A, B, F> Unpin for DelayRemove<A, B, F> where A: Unpin + SignalVec {}

impl<S, A, F> SignalVec for DelayRemove<S, A, F>
    where S: SignalVec,
          A: Future<Output = ()>,
          F: FnMut(&S::Item) -> A {
    type Item = S::Item;

    // TODO this can probably be implemented more efficiently
    fn poll_vec_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        unsafe_project!(self => {
            pin signal,
            mut futures,
            mut pending,
            mut callback,
        });

        if let Some(diff) = pending.pop_front() {
            return Poll::Ready(Some(diff));
        }

        let mut new_pending = PendingBuilder::new();

        // TODO maybe it should check the futures first, before checking the signalvec ?
        let done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(waker)) {
                None => true,
                Some(Poll::Ready(None)) => {
                    Pin::set(signal, None);
                    true
                },
                Some(Poll::Ready(Some(change))) => {
                    match change {
                        VecDiff::Replace { values } => {
                            // TODO what if it has futures but the futures are all done ?
                            if futures.len() == 0 {
                                *futures = values.iter().map(|value| DelayRemoveState::new(callback(value))).collect();
                                new_pending.push(VecDiff::Replace { values });

                            // TODO resize the capacity of `self.futures` somehow ?
                            } else {
                                Self::remove_existing_futures(futures, &mut new_pending, waker);

                                // TODO can this be made more efficient (e.g. using extend) ?
                                for value in values {
                                    let state = DelayRemoveState::new(callback(&value));
                                    futures.push(state);
                                    new_pending.push(VecDiff::Push { value });
                                }
                            }
                        },

                        VecDiff::InsertAt { index, value } => {
                            let index = Self::find_index(futures, index).unwrap_or_else(|| futures.len());
                            let state = DelayRemoveState::new(callback(&value));
                            futures.insert(index, state);
                            new_pending.push(VecDiff::InsertAt { index, value });
                        },

                        VecDiff::Push { value } => {
                            let state = DelayRemoveState::new(callback(&value));
                            futures.push(state);
                            new_pending.push(VecDiff::Push { value });
                        },

                        VecDiff::UpdateAt { index, value } => {
                            // TODO what about removing the old future ?
                            let index = Self::find_index(futures, index).expect("Could not find value");
                            let state = DelayRemoveState::new(callback(&value));
                            futures[index] = state;
                            new_pending.push(VecDiff::UpdateAt { index, value });
                        },

                        // TODO test this
                        // TODO should this be treated as a removal + insertion ?
                        VecDiff::Move { old_index, new_index } => {
                            let old_index = Self::find_index(futures, old_index).expect("Could not find value");

                            let state = futures.remove(old_index);

                            let new_index = Self::find_index(futures, new_index).unwrap_or_else(|| futures.len());

                            futures.insert(new_index, state);

                            new_pending.push(VecDiff::Move { old_index, new_index });
                        },

                        VecDiff::RemoveAt { index } => {
                            let index = Self::find_index(futures, index).expect("Could not find value");

                            if Self::should_remove(&mut futures[index], waker) {
                                new_pending.push(Self::remove_index(futures, index));
                            }
                        },

                        VecDiff::Pop {} => {
                            let index = Self::find_last_index(futures).expect("Cannot pop from empty vec");

                            if Self::should_remove(&mut futures[index], waker) {
                                new_pending.push(Self::remove_index(futures, index));
                            }
                        },

                        VecDiff::Clear {} => {
                            Self::remove_existing_futures(futures, &mut new_pending, waker);
                        },
                    }

                    continue;
                },
                Some(Poll::Pending) => {
                    false
                },
            };
        };

        let mut pending_removals = false;

        let mut indexes = vec![];

        // TODO make this more efficient (e.g. using a similar strategy as FuturesUnordered)
        for (index, state) in futures.iter_mut().enumerate() {
            if state.is_removing {
                if state.future.as_mut().poll(waker).is_ready() {
                    indexes.push(index);

                } else {
                    pending_removals = true;
                }
            }
        }

        // This uses rev so that way it will return VecDiff::Pop in more situations
        for index in indexes.into_iter().rev() {
            new_pending.push(Self::remove_index(futures, index));
        }

        if let Some(first) = new_pending.first {
            *pending = new_pending.rest;
            Poll::Ready(Some(first))

        } else if done && !pending_removals {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}


// TODO verify that this is correct
mod mutable_vec {
    use super::{SignalVec, VecDiff};
    use std::pin::Pin;
    use std::marker::Unpin;
    use std::fmt;
    use std::ops::Deref;
    use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
    use futures_channel::mpsc;
    use futures_core::Poll;
    use futures_core::task::LocalWaker;
    use futures_util::stream::StreamExt;
    use serde::{Serialize, Deserialize, Serializer, Deserializer};


    #[derive(Debug)]
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


    #[derive(Debug)]
    pub struct MutableVecLockRef<'a, A> where A: 'a {
        lock: RwLockReadGuard<'a, MutableVecState<A>>,
    }

    impl<'a, A> Deref for MutableVecLockRef<'a, A> {
        type Target = [A];

        #[inline]
        fn deref(&self) -> &Self::Target {
            &self.lock.values
        }
    }


    #[derive(Debug)]
    pub struct MutableVecLockMut<'a, A> where A: 'a {
        lock: RwLockWriteGuard<'a, MutableVecState<A>>,
    }

    impl<'a, A> MutableVecLockMut<'a, A> {
        #[inline]
        pub fn pop(&mut self) -> Option<A> {
            self.lock.pop()
        }

        #[inline]
        pub fn remove(&mut self, index: usize) -> A {
            self.lock.remove(index)
        }

        #[inline]
        pub fn clear(&mut self) {
            self.lock.clear()
        }

        #[inline]
        pub fn move_from_to(&mut self, old_index: usize, new_index: usize) {
            self.lock.move_from_to(old_index, new_index);
        }

        #[inline]
        pub fn retain<F>(&mut self, f: F) where F: FnMut(&A) -> bool {
            self.lock.retain(f)
        }
    }

    impl<'a, A> MutableVecLockMut<'a, A> where A: Copy {
        #[inline]
        pub fn push(&mut self, value: A) {
            self.lock.push_copy(value)
        }

        #[inline]
        pub fn insert(&mut self, index: usize, value: A) {
            self.lock.insert_copy(index, value)
        }

        // TODO replace this with something else, like entry or IndexMut or whatever
        #[inline]
        pub fn set(&mut self, index: usize, value: A) {
            self.lock.set_copy(index, value)
        }

        #[inline]
        pub fn replace(&mut self, values: Vec<A>) {
            self.lock.replace_copy(values)
        }
    }

    impl<'a, A> MutableVecLockMut<'a, A> where A: Clone {
        #[inline]
        pub fn push_cloned(&mut self, value: A) {
            self.lock.push_clone(value)
        }

        #[inline]
        pub fn insert_cloned(&mut self, index: usize, value: A) {
            self.lock.insert_clone(index, value)
        }

        // TODO replace this with something else, like entry or IndexMut or whatever
        #[inline]
        pub fn set_cloned(&mut self, index: usize, value: A) {
            self.lock.set_clone(index, value)
        }

        #[inline]
        pub fn replace_cloned(&mut self, values: Vec<A>) {
            self.lock.replace_clone(values)
        }
    }

    impl<'a, A> Deref for MutableVecLockMut<'a, A> {
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

        // TODO return Result ?
        #[inline]
        pub fn lock_ref(&self) -> MutableVecLockRef<A> {
            MutableVecLockRef {
                lock: self.0.read().unwrap(),
            }
        }

        // TODO return Result ?
        #[inline]
        pub fn lock_mut(&self) -> MutableVecLockMut<A> {
            MutableVecLockMut {
                lock: self.0.write().unwrap(),
            }
        }
    }

    impl<A: Copy> MutableVec<A> {
        #[inline]
        pub fn signal_vec(&self) -> MutableSignalVec<A> {
            self.0.write().unwrap().signal_vec_copy()
        }
    }

    impl<A: Clone> MutableVec<A> {
        #[inline]
        pub fn signal_vec_cloned(&self) -> MutableSignalVec<A> {
            self.0.write().unwrap().signal_vec_clone()
        }
    }

    impl<A> fmt::Debug for MutableVec<A> where A: fmt::Debug {
        fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
            let state = self.0.read().unwrap();

            fmt.debug_tuple("MutableVec")
                .field(&state.values)
                .finish()
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


    #[derive(Debug)]
    #[must_use = "SignalVecs do nothing unless polled"]
    pub struct MutableSignalVec<A> {
        receiver: mpsc::UnboundedReceiver<VecDiff<A>>,
    }

    impl<A> Unpin for MutableSignalVec<A> {}

    impl<A> SignalVec for MutableSignalVec<A> {
        type Item = A;

        #[inline]
        fn poll_vec_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
            self.receiver.poll_next_unpin(waker)
        }
    }
}

pub use self::mutable_vec::*;
