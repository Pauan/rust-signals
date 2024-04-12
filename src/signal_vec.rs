use std::iter::Sum;
use std::collections::VecDeque;
use std::pin::Pin;
use std::marker::Unpin;
use std::cmp::Ordering;
use std::future::Future;
use std::task::{Poll, Context};
use futures_core::Stream;
use futures_util::stream;
use futures_util::stream::StreamExt;
use pin_project::pin_project;

use crate::signal::{Signal, Mutable, ReadOnlyMutable};


// TODO make this non-exhaustive
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

    pub fn apply_to_vec(self, vec: &mut Vec<A>) {
        match self {
            VecDiff::Replace { values } => {
                *vec = values;
            },
            VecDiff::InsertAt { index, value } => {
                vec.insert(index, value);
            },
            VecDiff::UpdateAt { index, value } => {
                vec[index] = value;
            },
            VecDiff::Push { value } => {
                vec.push(value);
            },
            VecDiff::RemoveAt { index } => {
                vec.remove(index);
            },
            VecDiff::Move { old_index, new_index } => {
                let value = vec.remove(old_index);
                vec.insert(new_index, value);
            },
            VecDiff::Pop {} => {
                vec.pop().unwrap();
            },
            VecDiff::Clear {} => {
                vec.clear();
            },
        }
    }
}


// TODO impl for AssertUnwindSafe ?
#[must_use = "SignalVecs do nothing unless polled"]
pub trait SignalVec {
    type Item;

    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>>;
}


// Copied from Future in the Rust stdlib
impl<'a, A> SignalVec for &'a mut A where A: ?Sized + SignalVec + Unpin {
    type Item = A::Item;

    #[inline]
    fn poll_vec_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        A::poll_vec_change(Pin::new(&mut **self), cx)
    }
}

// Copied from Future in the Rust stdlib
impl<A> SignalVec for Box<A> where A: ?Sized + SignalVec + Unpin {
    type Item = A::Item;

    #[inline]
    fn poll_vec_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        A::poll_vec_change(Pin::new(&mut *self), cx)
    }
}

// Copied from Future in the Rust stdlib
impl<A> SignalVec for Pin<A>
    where A: Unpin + ::std::ops::DerefMut,
          A::Target: SignalVec {
    type Item = <<A as ::std::ops::Deref>::Target as SignalVec>::Item;

    #[inline]
    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        Pin::get_mut(self).as_mut().poll_vec_change(cx)
    }
}


// TODO Seal this
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

    /// Chains two `SignalVec`s together.
    ///
    /// The output `SignalVec` will contain all of the items in `self`,
    /// followed by all of the items in `other`.
    ///
    /// This behaves just like the [`Iterator::chain`](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.chain)
    /// method, except it will automatically keep the output `SignalVec` in sync with the
    /// two input `SignalVec`s.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use futures_signals::signal_vec::{always, SignalVecExt};
    /// # let left = always(vec![1, 2, 3]);
    /// # let right = always(vec![4, 5, 6]);
    /// // left = [1, 2, 3]
    /// // right = [4, 5, 6]
    /// let chained = left.chain(right);
    /// // chained = [1, 2, 3, 4, 5, 6]
    /// ```
    ///
    /// # Performance
    ///
    /// This is a fairly efficient method: it is *guaranteed* constant time, regardless of how big `self` or `other` are.
    ///
    /// In addition, it does not do any heap allocation, the only internal state it keeps is 2 `usize`.
    ///
    /// The only exception is when `self` or `other` notifies with `VecDiff::Replace` or `VecDiff::Clear`,
    /// in which case it is linear time (and it heap allocates a single
    /// [`VecDeque`](https://doc.rust-lang.org/std/collections/struct.VecDeque.html)).
    #[inline]
    fn chain<S>(self, other: S) -> Chain<Self, S>
        where S: SignalVec<Item = Self::Item>,
              Self: Sized {
        Chain {
            left: Some(self),
            right: Some(other),
            left_len: 0,
            right_len: 0,
            pending: VecDeque::new(),
        }
    }

    #[inline]
    fn to_signal_map<A, F>(self, callback: F) -> ToSignalMap<Self, F>
        where F: FnMut(&[Self::Item]) -> A,
              Self: Sized {
        ToSignalMap {
            signal: Some(self),
            first: true,
            values: vec![],
            callback,
        }
    }

    #[inline]
    fn to_signal_cloned(self) -> ToSignalCloned<Self>
        where Self::Item: Clone,
              Self: Sized {
        ToSignalCloned {
            signal: self.to_signal_map(|x| x.to_vec()),
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
    fn filter_map<A, F>(self, callback: F) -> FilterMap<Self, F>
        where F: FnMut(Self::Item) -> Option<A>,
              Self: Sized {
        FilterMap {
            indexes: vec![],
            signal: self,
            callback,
        }
    }

    // TODO replace with to_signal_map ?
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

    /// Flattens a `SignalVec<SignalVec<A>>` into a `SignalVec<A>`.
    #[inline]
    fn flatten(self) -> Flatten<Self>
        where Self::Item: SignalVec,
              Self: Sized {
        Flatten {
            signal: Some(self),
            inner: vec![],
            pending: VecDeque::new(),
        }
    }

    #[inline]
    #[track_caller]
    #[cfg(feature = "debug")]
    fn debug(self) -> SignalVecDebug<Self> where Self: Sized, Self::Item: std::fmt::Debug {
        SignalVecDebug {
            signal: self,
            location: std::panic::Location::caller(),
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
    /// # Requirements
    ///
    /// It is invalid for the sort order to dynamically change. If dynamic sorting is needed, you can use
    /// [`map_signal`](#method.map_signal):
    ///
    /// ```rust
    /// # use futures_signals::{signal, signal_vec};
    /// # use futures_signals::signal_vec::SignalVecExt;
    /// # let input = signal_vec::always(vec![3, 1, 6, 2, 0, 4, 5, 8, 9, 7]);
    /// # fn returns_a_signal(x: u32) -> impl signal::Signal<Item = u32> { signal::always(x) }
    /// let sorted = input
    ///     .map_signal(|x| {
    ///         returns_a_signal(x)
    ///     })
    ///     .sort_by_cloned(|x, y| {
    ///         // ...
    ///         # std::cmp::Ordering::Equal
    ///     });
    /// ```
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

    // TODO replace with to_signal_map ?
    #[inline]
    fn len(self) -> Len<Self> where Self: Sized {
        Len {
            signal: Some(self),
            first: true,
            len: 0,
        }
    }

    #[inline]
    fn is_empty(self) -> IsEmpty<Self> where Self: Sized {
        IsEmpty {
            len: self.len(),
            old: None,
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
    fn poll_vec_change_unpin(&mut self, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> where Self: Unpin + Sized {
        Pin::new(self).poll_vec_change(cx)
    }

    #[inline]
    fn boxed<'a>(self) -> Pin<Box<dyn SignalVec<Item = Self::Item> + Send + 'a>>
        where Self: Sized + Send + 'a {
        Box::pin(self)
    }

    #[inline]
    fn boxed_local<'a>(self) -> Pin<Box<dyn SignalVec<Item = Self::Item> + 'a>>
        where Self: Sized + 'a {
        Box::pin(self)
    }
}

// TODO why is this ?Sized
impl<T: ?Sized> SignalVecExt for T where T: SignalVec {}


/// An owned dynamically typed [`SignalVec`].
///
/// This is useful if you don't know the static type, or if you need
/// indirection.
pub type BoxSignalVec<'a, T> = Pin<Box<dyn SignalVec<Item = T> + Send + 'a>>;

/// Same as [`BoxSignalVec`], but without the `Send` requirement.
pub type LocalBoxSignalVec<'a, T> = Pin<Box<dyn SignalVec<Item = T> + 'a>>;


#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct Always<A> {
    values: Option<Vec<A>>,
}

impl<A> Unpin for Always<A> {}

impl<A> SignalVec for Always<A> {
    type Item = A;

    fn poll_vec_change(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        Poll::Ready(self.values.take().map(|values| VecDiff::Replace { values }))
    }
}

/// Converts a `Vec<A>` into a `SignalVec<Item = A>`
///
/// This has no performance cost.
#[inline]
pub fn always<A>(values: Vec<A>) -> Always<A> {
    Always {
        values: Some(values),
    }
}


#[pin_project(project = StreamSignalVecProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct StreamSignalVec<S> {
    #[pin]
    stream: S,
}

impl<S> SignalVec for StreamSignalVec<S> where S: Stream {
    type Item = S::Item;

    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let StreamSignalVecProj { stream } = self.project();

        stream.poll_next(cx).map(|some| some.map(|value| VecDiff::Push { value }))
    }
}

/// Converts a `Stream<Item = A>` into a `SignalVec<Item = A>`
///
/// The values are always pushed to the end of the SignalVec. This has no performance cost.
#[inline]
pub fn from_stream<S>(stream: S) -> StreamSignalVec<S> {
    StreamSignalVec {
        stream,
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct ForEach<A, B, C> {
    #[pin]
    inner: stream::ForEach<SignalVecStream<A>, B, C>,
}

impl<A, B, C> Future for ForEach<A, B, C>
    where A: SignalVec,
          B: Future<Output = ()>,
          C: FnMut(VecDiff<A::Item>) -> B {
    type Output = ();

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}


#[pin_project(project = MapProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct Map<A, B> {
    #[pin]
    signal: A,
    callback: B,
}

impl<A, B, F> SignalVec for Map<A, F>
    where A: SignalVec,
          F: FnMut(A::Item) -> B {
    type Item = B;

    // TODO should this inline ?
    #[inline]
    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let MapProj { signal, callback } = self.project();

        signal.poll_vec_change(cx).map(|some| some.map(|change| change.map(|value| callback(value))))
    }
}


// TODO investigate whether it's better to process left or right first
#[pin_project(project = ChainProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct Chain<A, B> where A: SignalVec {
    #[pin]
    left: Option<A>,
    #[pin]
    right: Option<B>,
    left_len: usize,
    right_len: usize,
    pending: VecDeque<VecDiff<A::Item>>,
}

impl<'a, A, B> ChainProj<'a, A, B> where A: SignalVec, B: SignalVec<Item = A::Item> {
    fn is_replace(change: Option<Poll<Option<VecDiff<A::Item>>>>) -> Result<Vec<A::Item>, Option<Poll<Option<VecDiff<A::Item>>>>> {
        match change {
            Some(Poll::Ready(Some(VecDiff::Replace { values }))) => Ok(values),
            _ => Err(change),
        }
    }

    fn is_clear(change: Option<Poll<Option<VecDiff<A::Item>>>>) -> Result<(), Option<Poll<Option<VecDiff<A::Item>>>>> {
        match change {
            Some(Poll::Ready(Some(VecDiff::Clear {}))) => Ok(()),
            _ => Err(change),
        }
    }

    fn process_replace(&mut self, mut left_values: Vec<A::Item>, cx: &mut Context) -> Option<VecDiff<A::Item>> {
        match Self::is_replace(self.right.as_mut().as_pin_mut().map(|s| s.poll_vec_change(cx))) {
            Ok(mut right_values) => {
                *self.left_len = left_values.len();
                *self.right_len = right_values.len();
                left_values.append(&mut right_values);
                Some(VecDiff::Replace { values: left_values })
            },
            Err(change) => {
                let removing = *self.left_len;
                let adding = left_values.len();

                *self.left_len = adding;

                let output = if *self.right_len == 0 {
                    Some(VecDiff::Replace { values: left_values })

                } else {
                    self.pending.reserve(removing + adding);

                    for index in (0..removing).rev() {
                        self.pending.push_back(VecDiff::RemoveAt { index });
                    }

                    for (index, value) in left_values.into_iter().enumerate() {
                        self.pending.push_back(VecDiff::InsertAt { index, value });
                    }

                    None
                };

                if let Some(change) = self.process_right_change(change) {
                    self.pending.push_back(change);
                }

                output
            },
        }
    }

    fn process_clear(&mut self, cx: &mut Context) -> Option<VecDiff<A::Item>> {
        match Self::is_clear(self.right.as_mut().as_pin_mut().map(|s| s.poll_vec_change(cx))) {
            Ok(()) => {
                *self.left_len = 0;
                *self.right_len = 0;
                Some(VecDiff::Clear {})
            },
            Err(change) => {
                let removing = *self.left_len;

                *self.left_len = 0;

                let output = if *self.right_len == 0 {
                    Some(VecDiff::Clear {})

                } else {
                    self.pending.reserve(removing);

                    for index in (0..removing).rev() {
                        self.pending.push_back(VecDiff::RemoveAt { index });
                    }

                    None
                };

                if let Some(change) = self.process_right_change(change) {
                    self.pending.push_back(change);
                }

                output
            },
        }
    }

    fn process_left(&mut self, cx: &mut Context) -> Option<VecDiff<A::Item>> {
        match self.left.as_mut().as_pin_mut().map(|s| s.poll_vec_change(cx)) {
            Some(Poll::Ready(Some(change))) => {
                match change {
                    VecDiff::Replace { values } => {
                        self.process_replace(values, cx)
                    },
                    VecDiff::InsertAt { index, value } => {
                        *self.left_len += 1;
                        Some(VecDiff::InsertAt { index, value })
                    },
                    VecDiff::UpdateAt { index, value } => {
                        Some(VecDiff::UpdateAt { index, value })
                    },
                    VecDiff::Move { old_index, new_index } => {
                        Some(VecDiff::Move { old_index, new_index })
                    },
                    VecDiff::RemoveAt { index } => {
                        *self.left_len -= 1;
                        Some(VecDiff::RemoveAt { index })
                    },
                    VecDiff::Push { value } => {
                        let index = *self.left_len;
                        *self.left_len += 1;

                        if *self.right_len == 0 {
                            Some(VecDiff::Push { value })

                        } else {
                            Some(VecDiff::InsertAt { index, value })
                        }
                    },
                    VecDiff::Pop {} => {
                        *self.left_len -= 1;

                        if *self.right_len == 0 {
                            Some(VecDiff::Pop {})

                        } else {
                            Some(VecDiff::RemoveAt { index: *self.left_len })
                        }
                    },
                    VecDiff::Clear {} => {
                        self.process_clear(cx)
                    },
                }
            },
            Some(Poll::Ready(None)) => {
                self.left.set(None);
                None
            },
            Some(Poll::Pending) => None,
            None => None,
        }
    }

    fn process_right_change(&mut self, change: Option<Poll<Option<VecDiff<A::Item>>>>) -> Option<VecDiff<A::Item>> {
        match change {
            Some(Poll::Ready(Some(change))) => {
                match change {
                    VecDiff::Replace { values } => {
                        let removing = *self.right_len;
                        let adding = values.len();

                        *self.right_len = adding;

                        if *self.left_len == 0 {
                            Some(VecDiff::Replace { values })

                        } else {
                            self.pending.reserve(removing + adding);

                            for _ in 0..removing {
                                self.pending.push_back(VecDiff::Pop {});
                            }

                            for value in values.into_iter() {
                                self.pending.push_back(VecDiff::Push { value });
                            }

                            None
                        }
                    },
                    VecDiff::InsertAt { index, value } => {
                        *self.right_len += 1;
                        Some(VecDiff::InsertAt { index: index + *self.left_len, value })
                    },
                    VecDiff::UpdateAt { index, value } => {
                        Some(VecDiff::UpdateAt { index: index + *self.left_len, value })
                    },
                    VecDiff::Move { old_index, new_index } => {
                        Some(VecDiff::Move {
                            old_index: old_index + *self.left_len,
                            new_index: new_index + *self.left_len,
                        })
                    },
                    VecDiff::RemoveAt { index } => {
                        *self.right_len -= 1;
                        Some(VecDiff::RemoveAt { index: index + *self.left_len })
                    },
                    VecDiff::Push { value } => {
                        *self.right_len += 1;
                        Some(VecDiff::Push { value })
                    },
                    VecDiff::Pop {} => {
                        *self.right_len -= 1;
                        Some(VecDiff::Pop {})
                    },
                    VecDiff::Clear {} => {
                        let removing = *self.right_len;

                        *self.right_len = 0;

                        if *self.left_len == 0 {
                            Some(VecDiff::Clear {})

                        } else {
                            self.pending.reserve(removing);

                            for _ in 0..removing {
                                self.pending.push_back(VecDiff::Pop {});
                            }

                            None
                        }
                    },
                }
            },
            Some(Poll::Ready(None)) => {
                self.right.set(None);
                None
            },
            Some(Poll::Pending) => None,
            None => None,
        }
    }

    fn process_right(&mut self, cx: &mut Context) -> Option<VecDiff<A::Item>> {
        let change = self.right.as_mut().as_pin_mut().map(|s| s.poll_vec_change(cx));
        self.process_right_change(change)
    }
}

impl<A, B> SignalVec for Chain<A, B>
    where A: SignalVec,
          B: SignalVec<Item = A::Item> {
    type Item = A::Item;

    #[inline]
    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let mut this = self.project();

        loop {
            if let Some(change) = this.pending.pop_front() {
                return Poll::Ready(Some(change));
            }

            if let Some(change) = this.process_left(cx) {
                return Poll::Ready(Some(change));

            } else if let Some(change) = this.pending.pop_front() {
                return Poll::Ready(Some(change));
            }

            if let Some(change) = this.process_right(cx) {
                return Poll::Ready(Some(change));

            } else if let Some(change) = this.pending.pop_front() {
                return Poll::Ready(Some(change));
            }

            if this.left.is_none() && this.right.is_none() {
                return Poll::Ready(None);

            } else {
                return Poll::Pending;
            }
        }
    }
}


#[derive(Debug)]
struct FlattenState<A> {
    // TODO is there a more efficient way to implement this ?
    signal_vec: Option<Pin<Box<A>>>,
    len: usize,
}

impl<A> FlattenState<A> where A: SignalVec {
    fn new(signal_vec: A) -> Self {
        Self {
            signal_vec: Some(Box::pin(signal_vec)),
            len: 0,
        }
    }

    fn update_len(&mut self, diff: &VecDiff<A::Item>) {
        match diff {
            VecDiff::Replace { values } => {
                self.len = values.len();
            },
            VecDiff::InsertAt { .. } | VecDiff::Push { .. } => {
                self.len += 1;
            },
            VecDiff::RemoveAt { .. } | VecDiff::Pop {} => {
                self.len -= 1;
            },
            VecDiff::Clear {} => {
                self.len = 0;
            },
            VecDiff::UpdateAt { .. } | VecDiff::Move { .. } => {},
        }
    }

    fn poll(&mut self, cx: &mut Context) -> Option<Poll<Option<VecDiff<A::Item>>>> {
        self.signal_vec.as_mut().map(|s| s.poll_vec_change_unpin(cx))
    }

    fn poll_values(&mut self, cx: &mut Context) -> Vec<A::Item> {
        let mut output = vec![];

        loop {
            match self.poll(cx) {
                Some(Poll::Ready(Some(diff))) => {
                    self.update_len(&diff);
                    diff.apply_to_vec(&mut output);
                },
                Some(Poll::Ready(None)) => {
                    self.signal_vec = None;
                    break;
                },
                Some(Poll::Pending) | None => {
                    break;
                },
            }
        }

        output
    }

    fn poll_pending(&mut self, cx: &mut Context, prev_len: usize, pending: &mut PendingBuilder<VecDiff<A::Item>>) -> bool {
        loop {
            return match self.poll(cx) {
                Some(Poll::Ready(Some(diff))) => {
                    let old_len = self.len;

                    self.update_len(&diff);

                    match diff {
                        VecDiff::Replace { values } => {
                            for index in (0..old_len).rev() {
                                pending.push(VecDiff::RemoveAt { index: prev_len + index });
                            }

                            for (index, value) in values.into_iter().enumerate() {
                                pending.push(VecDiff::InsertAt { index: prev_len + index, value });
                            }
                        },
                        VecDiff::InsertAt { index, value } => {
                            pending.push(VecDiff::InsertAt { index: prev_len + index, value });
                        },
                        VecDiff::UpdateAt { index, value } => {
                            pending.push(VecDiff::UpdateAt { index: prev_len + index, value });
                        },
                        VecDiff::RemoveAt { index } => {
                            pending.push(VecDiff::RemoveAt { index: prev_len + index });
                        },
                        VecDiff::Move { old_index, new_index } => {
                            pending.push(VecDiff::Move { old_index: prev_len + old_index, new_index: prev_len + new_index });
                        },
                        VecDiff::Push { value } => {
                            pending.push(VecDiff::InsertAt { index: prev_len + old_len, value });
                        },
                        VecDiff::Pop {} => {
                            pending.push(VecDiff::RemoveAt { index: prev_len + (old_len - 1) });
                        },
                        VecDiff::Clear {} => {
                            for index in (0..old_len).rev() {
                                pending.push(VecDiff::RemoveAt { index: prev_len + index });
                            }
                        },
                    }

                    continue;
                },
                Some(Poll::Ready(None)) => {
                    self.signal_vec = None;
                    true
                },
                Some(Poll::Pending) => {
                    false
                },
                None => {
                    true
                },
            };
        }
    }
}

#[pin_project(project = FlattenProj)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct Flatten<A> where A: SignalVec, A::Item: SignalVec {
    #[pin]
    signal: Option<A>,
    inner: Vec<FlattenState<A::Item>>,
    pending: VecDeque<VecDiff<<A::Item as SignalVec>::Item>>,
}

impl<A> std::fmt::Debug for Flatten<A>
    where A: SignalVec + std::fmt::Debug,
          A::Item: SignalVec + std::fmt::Debug,
          <A::Item as SignalVec>::Item: std::fmt::Debug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flatten")
            .field("signal", &self.signal)
            .field("inner", &self.inner)
            .field("pending", &self.pending)
            .finish()
    }
}

impl<A> SignalVec for Flatten<A>
    where A: SignalVec,
          A::Item: SignalVec {
    type Item = <A::Item as SignalVec>::Item;

    // TODO implement this more efficiently
    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let mut this = self.project();

        if let Some(diff) = this.pending.pop_front() {
            return Poll::Ready(Some(diff));
        }

        let mut pending: PendingBuilder<VecDiff<<A::Item as SignalVec>::Item>> = PendingBuilder::new();

        let top_done = loop {
            break match this.signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(cx)) {
                Some(Poll::Ready(Some(diff))) => {
                    match diff {
                        VecDiff::Replace { values } => {
                            *this.inner = values.into_iter().map(FlattenState::new).collect();

                            let values = this.inner.iter_mut()
                                .flat_map(|state| state.poll_values(cx))
                                .collect();

                            return Poll::Ready(Some(VecDiff::Replace { values }));
                        },
                        VecDiff::InsertAt { index, value } => {
                            this.inner.insert(index, FlattenState::new(value));
                        },
                        VecDiff::UpdateAt { index, value } => {
                            this.inner[index] = FlattenState::new(value);
                        },
                        VecDiff::RemoveAt { index } => {
                            this.inner.remove(index);
                        },
                        VecDiff::Move { old_index, new_index } => {
                            let value = this.inner.remove(old_index);
                            this.inner.insert(new_index, value);
                        },
                        VecDiff::Push { value } => {
                            this.inner.push(FlattenState::new(value));
                        },
                        VecDiff::Pop {} => {
                            this.inner.pop().unwrap();
                        },
                        VecDiff::Clear {} => {
                            this.inner.clear();
                            return Poll::Ready(Some(VecDiff::Clear {}));
                        },
                    }

                    continue;
                },
                Some(Poll::Ready(None)) => {
                    this.signal.set(None);
                    true
                },
                Some(Poll::Pending) => {
                    false
                },
                None => {
                    true
                },
            };
        };

        let mut inner_done = true;

        let mut prev_len = 0;

        for state in this.inner.iter_mut() {
            let done = state.poll_pending(cx, prev_len, &mut pending);

            if !done {
                inner_done = false;
            }

            prev_len += state.len;
        }

        if let Some(first) = pending.first {
            *this.pending = pending.rest;
            Poll::Ready(Some(first))

        } else if inner_done && top_done {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}


#[pin_project]
#[must_use = "Signals do nothing unless polled"]
pub struct ToSignalCloned<A> where A: SignalVec {
    #[pin]
    signal: ToSignalMap<A, fn(&[A::Item]) -> Vec<A::Item>>,
}

impl<A> std::fmt::Debug for ToSignalCloned<A> where A: SignalVec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ToSignalCloned { ... }")
    }
}

impl<A> Signal for ToSignalCloned<A>
    where A: SignalVec {
    type Item = Vec<A::Item>;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().signal.poll_change(cx)
    }
}


#[pin_project(project = ToSignalMapProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct ToSignalMap<A, B> where A: SignalVec {
    #[pin]
    signal: Option<A>,
    // This is needed because a Signal must always have a value, even if the SignalVec is empty
    first: bool,
    values: Vec<A::Item>,
    callback: B,
}

impl<A, B, F> Signal for ToSignalMap<A, F>
    where A: SignalVec,
          F: FnMut(&[A::Item]) -> B {
    type Item = B;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let ToSignalMapProj { mut signal, first, values, callback } = self.project();

        let mut changed = false;

        let done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(cx)) {
                None => {
                    true
                },
                Some(Poll::Ready(None)) => {
                    signal.set(None);
                    true
                },
                Some(Poll::Ready(Some(change))) => {
                    match change {
                        VecDiff::Replace { values: new_values } => {
                            // TODO only set changed if the values are different ?
                            *values = new_values;
                        },

                        VecDiff::InsertAt { index, value } => {
                            values.insert(index, value);
                        },

                        VecDiff::UpdateAt { index, value } => {
                            // TODO only set changed if the value is different ?
                            values[index] = value;
                        },

                        VecDiff::RemoveAt { index } => {
                            values.remove(index);
                        },

                        VecDiff::Move { old_index, new_index } => {
                            let old = values.remove(old_index);
                            values.insert(new_index, old);
                        },

                        VecDiff::Push { value } => {
                            values.push(value);
                        },

                        VecDiff::Pop {} => {
                            values.pop().unwrap();
                        },

                        VecDiff::Clear {} => {
                            // TODO only set changed if the len is different ?
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
            Poll::Ready(Some(callback(&values)))

        } else if done {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}


#[pin_project(project = EnumerateProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct Enumerate<A> {
    #[pin]
    signal: A,
    mutables: Vec<Mutable<Option<usize>>>,
}

impl<A> SignalVec for Enumerate<A> where A: SignalVec {
    type Item = (ReadOnlyMutable<Option<usize>>, A::Item);

    #[inline]
    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
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

        let EnumerateProj { signal, mutables } = self.project();

        // TODO use map ?
        match signal.poll_vec_change(cx) {
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

#[pin_project(project = MapSignalProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct MapSignal<A, B, F> where B: Signal {
    #[pin]
    signal: Option<A>,
    // TODO is there a more efficient way to implement this ?
    signals: Vec<Option<Pin<Box<B>>>>,
    pending: VecDeque<VecDiff<B::Item>>,
    callback: F,
}

impl<A, B, F> SignalVec for MapSignal<A, B, F>
    where A: SignalVec,
          B: Signal,
          F: FnMut(A::Item) -> B {
    type Item = B::Item;

    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let MapSignalProj { mut signal, signals, pending, callback } = self.project();

        if let Some(diff) = pending.pop_front() {
            return Poll::Ready(Some(diff));
        }

        let mut new_pending = PendingBuilder::new();

        let done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(cx)) {
                None => {
                    true
                },
                Some(Poll::Ready(None)) => {
                    signal.set(None);
                    true
                },
                Some(Poll::Ready(Some(change))) => {
                    new_pending.push(match change {
                        VecDiff::Replace { values } => {
                            *signals = Vec::with_capacity(values.len());

                            VecDiff::Replace {
                                values: values.into_iter().map(|value| {
                                    let mut signal = Box::pin(callback(value));
                                    let poll = unwrap(signal.as_mut().poll_change(cx));
                                    signals.push(Some(signal));
                                    poll
                                }).collect()
                            }
                        },

                        VecDiff::InsertAt { index, value } => {
                            let mut signal = Box::pin(callback(value));
                            let poll = unwrap(signal.as_mut().poll_change(cx));
                            signals.insert(index, Some(signal));
                            VecDiff::InsertAt { index, value: poll }
                        },

                        VecDiff::UpdateAt { index, value } => {
                            let mut signal = Box::pin(callback(value));
                            let poll = unwrap(signal.as_mut().poll_change(cx));
                            signals[index] = Some(signal);
                            VecDiff::UpdateAt { index, value: poll }
                        },

                        VecDiff::Push { value } => {
                            let mut signal = Box::pin(callback(value));
                            let poll = unwrap(signal.as_mut().poll_change(cx));
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
            match signal.as_mut().map(|s| s.as_mut().poll_change(cx)) {
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

#[pin_project(project = FilterSignalClonedProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct FilterSignalCloned<A, B, F> where A: SignalVec {
    #[pin]
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

impl<A, B, F> SignalVec for FilterSignalCloned<A, B, F>
    where A: SignalVec,
          A::Item: Clone,
          B: Signal<Item = bool>,
          F: FnMut(&A::Item) -> B {
    type Item = A::Item;

    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let FilterSignalClonedProj { mut signal, signals, pending, callback } = self.project();

        if let Some(diff) = pending.pop_front() {
            return Poll::Ready(Some(diff));
        }

        let mut new_pending = PendingBuilder::new();

        // TODO maybe it should check the filter signals first, before checking the signalvec ?
        let done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(cx)) {
                None => true,
                Some(Poll::Ready(None)) => {
                    signal.set(None);
                    true
                },
                Some(Poll::Ready(Some(change))) => {
                    new_pending.push(match change {
                        VecDiff::Replace { values } => {
                            *signals = Vec::with_capacity(values.len());

                            VecDiff::Replace {
                                values: values.into_iter().filter(|value| {
                                    let mut signal = Box::pin(callback(value));
                                    let poll = unwrap(signal.as_mut().poll_change(cx));

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
                            let poll = unwrap(signal.as_mut().poll_change(cx));

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
                            let new_poll = unwrap(signal.as_mut().poll_change(cx));

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
                            let poll = unwrap(signal.as_mut().poll_change(cx));

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
                match state.signal.as_mut().map(|s| s.as_mut().poll_change(cx)) {
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


#[pin_project(project = IsEmptyProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct IsEmpty<A> {
    #[pin]
    len: Len<A>,
    old: Option<bool>,
}

impl<A> Signal for IsEmpty<A> where A: SignalVec {
    type Item = bool;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let IsEmptyProj { len, old } = self.project();

        match len.poll_change(cx) {
            Poll::Ready(Some(len)) => {
                let new = Some(len == 0);

                if *old != new {
                    *old = new;
                    Poll::Ready(new)

                } else {
                    Poll::Pending
                }
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}


#[pin_project(project = LenProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Len<A> {
    #[pin]
    signal: Option<A>,
    first: bool,
    len: usize,
}

impl<A> Signal for Len<A> where A: SignalVec {
    type Item = usize;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let LenProj { mut signal, first, len } = self.project();

        let mut changed = false;

        let done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(cx)) {
                None => {
                    true
                },
                Some(Poll::Ready(None)) => {
                    signal.set(None);
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


#[pin_project]
#[derive(Debug)]
#[must_use = "Streams do nothing unless polled"]
pub struct SignalVecStream<A> {
    #[pin]
    signal: A,
}

impl<A: SignalVec> Stream for SignalVecStream<A> {
    type Item = VecDiff<A::Item>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().signal.poll_vec_change(cx)
    }
}


fn find_index(indexes: &[bool], index: usize) -> usize {
    indexes[0..index].into_iter().filter(|x| **x).count()
}

fn poll_filter_map<A, S, F>(indexes: &mut Vec<bool>, mut signal: Pin<&mut S>, cx: &mut Context, mut callback: F) -> Poll<Option<VecDiff<A>>>
    where S: SignalVec,
          F: FnMut(S::Item) -> Option<A> {

    loop {
        return match signal.as_mut().poll_vec_change(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(change)) => match change {
                VecDiff::Replace { values } => {
                    *indexes = Vec::with_capacity(values.len());

                    Poll::Ready(Some(VecDiff::Replace {
                        values: values.into_iter().filter_map(|value| {
                            let value = callback(value);
                            indexes.push(value.is_some());
                            value
                        }).collect()
                    }))
                },

                VecDiff::InsertAt { index, value } => {
                    if let Some(value) = callback(value) {
                        indexes.insert(index, true);
                        Poll::Ready(Some(VecDiff::InsertAt { index: find_index(indexes, index), value }))

                    } else {
                        indexes.insert(index, false);
                        continue;
                    }
                },

                VecDiff::UpdateAt { index, value } => {
                    if let Some(value) = callback(value) {
                        if indexes[index] {
                            Poll::Ready(Some(VecDiff::UpdateAt { index: find_index(indexes, index), value }))

                        } else {
                            indexes[index] = true;
                            Poll::Ready(Some(VecDiff::InsertAt { index: find_index(indexes, index), value }))
                        }

                    } else {
                        if indexes[index] {
                            indexes[index] = false;
                            Poll::Ready(Some(VecDiff::RemoveAt { index: find_index(indexes, index) }))

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
                            old_index: find_index(indexes, old_index),
                            new_index: find_index(indexes, new_index),
                        }))

                    } else {
                        indexes.insert(new_index, false);
                        continue;
                    }
                },

                VecDiff::RemoveAt { index } => {
                    if indexes.remove(index) {
                        Poll::Ready(Some(VecDiff::RemoveAt { index: find_index(indexes, index) }))

                    } else {
                        continue;
                    }
                },

                VecDiff::Push { value } => {
                    if let Some(value) = callback(value) {
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


#[pin_project(project = FilterProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct Filter<A, B> {
    // TODO use a bit vec for smaller size
    indexes: Vec<bool>,
    #[pin]
    signal: A,
    callback: B,
}

impl<A, F> SignalVec for Filter<A, F>
    where A: SignalVec,
          F: FnMut(&A::Item) -> bool {
    type Item = A::Item;

    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let FilterProj { indexes, signal, callback } = self.project();

        poll_filter_map(indexes, signal, cx, move |value| {
            if callback(&value) {
                Some(value)
            } else {
                None
            }
        })
    }
}


#[pin_project(project = FilterMapProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct FilterMap<S, F> {
    // TODO use a bit vec for smaller size
    indexes: Vec<bool>,
    #[pin]
    signal: S,
    callback: F,
}

impl<S, A, F> SignalVec for FilterMap<S, F>
    where S: SignalVec,
          F: FnMut(S::Item) -> Option<A> {
    type Item = A;

    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let FilterMapProj { indexes, signal, callback } = self.project();
        poll_filter_map(indexes, signal, cx, callback)
    }
}


#[pin_project(project = SumSignalProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct SumSignal<A> where A: SignalVec {
    #[pin]
    signal: Option<A>,
    first: bool,
    values: Vec<A::Item>,
}

impl<A> Signal for SumSignal<A>
    where A: SignalVec,
          A::Item: for<'a> Sum<&'a A::Item> {
    type Item = A::Item;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let SumSignalProj { mut signal, first, values } = self.project();

        let mut changed = false;

        let done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(cx)) {
                None => {
                    true
                },
                Some(Poll::Ready(None)) => {
                    signal.set(None);
                    true
                },
                Some(Poll::Ready(Some(change))) => {
                    match change {
                        VecDiff::Replace { values: new_values } => {
                            // TODO only mark changed if the values are different
                            *values = new_values;
                        },

                        VecDiff::InsertAt { index, value } => {
                            // TODO only mark changed if the value isn't 0
                            values.insert(index, value);
                        },

                        VecDiff::Push { value } => {
                            // TODO only mark changed if the value isn't 0
                            values.push(value);
                        },

                        VecDiff::UpdateAt { index, value } => {
                            // TODO only mark changed if the value is different
                            values[index] = value;
                        },

                        VecDiff::Move { old_index, new_index } => {
                            let value = values.remove(old_index);
                            values.insert(new_index, value);
                            // Moving shouldn't recalculate the sum
                            continue;
                        },

                        VecDiff::RemoveAt { index } => {
                            // TODO only mark changed if the value isn't 0
                            values.remove(index);
                        },

                        VecDiff::Pop {} => {
                            // TODO only mark changed if the value isn't 0
                            values.pop().unwrap();
                        },

                        VecDiff::Clear {} => {
                            // TODO only mark changed if the len is different
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


#[pin_project(project = SignalVecDebugProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
#[cfg(feature = "debug")]
pub struct SignalVecDebug<A> {
    #[pin]
    signal: A,
    location: &'static std::panic::Location<'static>,
}

#[cfg(feature = "debug")]
impl<A> SignalVec for SignalVecDebug<A> where A: SignalVec, A::Item: std::fmt::Debug {
    type Item = A::Item;

    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let SignalVecDebugProj { signal, location } = self.project();

        let poll = signal.poll_vec_change(cx);

        log::trace!("[{}] {:#?}", location, poll);

        poll
    }
}


#[pin_project(project = SortByClonedProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct SortByCloned<A, B> where A: SignalVec {
    pending: Option<VecDiff<A::Item>>,
    values: Vec<A::Item>,
    indexes: Vec<usize>,
    #[pin]
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

// TODO implementation of this for Copy
impl<A, F> SignalVec for SortByCloned<A, F>
    where A: SignalVec,
          F: FnMut(&A::Item, &A::Item) -> Ordering,
          A::Item: Clone {
    type Item = A::Item;

    // TODO figure out a faster implementation of this
    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let SortByClonedProj { pending, values, indexes, mut signal, compare } = self.project();

        match pending.take() {
            Some(value) => Poll::Ready(Some(value)),
            None => loop {
                return match signal.as_mut().poll_vec_change(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Ready(Some(change)) => match change {
                        VecDiff::Replace { values: new_values } => {
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

#[pin_project(project = DelayRemoveProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct DelayRemove<A, B, F> where A: SignalVec {
    #[pin]
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

    fn should_remove(state: &mut DelayRemoveState<A>, cx: &mut Context) -> bool {
        assert!(!state.is_removing);

        if state.future.as_mut().poll(cx).is_ready() {
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

    fn remove_existing_futures(futures: &mut Vec<DelayRemoveState<A>>, pending: &mut PendingBuilder<VecDiff<S::Item>>, cx: &mut Context) {
        let mut indexes = vec![];

        for (index, future) in futures.iter_mut().enumerate() {
            if !future.is_removing {
                if Self::should_remove(future, cx) {
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

impl<S, A, F> SignalVec for DelayRemove<S, A, F>
    where S: SignalVec,
          A: Future<Output = ()>,
          F: FnMut(&S::Item) -> A {
    type Item = S::Item;

    // TODO this can probably be implemented more efficiently
    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let DelayRemoveProj { mut signal, futures, pending, callback } = self.project();

        if let Some(diff) = pending.pop_front() {
            return Poll::Ready(Some(diff));
        }

        let mut new_pending = PendingBuilder::new();

        // TODO maybe it should check the futures first, before checking the signalvec ?
        let done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(cx)) {
                None => true,
                Some(Poll::Ready(None)) => {
                    signal.set(None);
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
                                Self::remove_existing_futures(futures, &mut new_pending, cx);

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

                            if Self::should_remove(&mut futures[index], cx) {
                                new_pending.push(Self::remove_index(futures, index));
                            }
                        },

                        VecDiff::Pop {} => {
                            let index = Self::find_last_index(futures).expect("Cannot pop from empty vec");

                            if Self::should_remove(&mut futures[index], cx) {
                                new_pending.push(Self::remove_index(futures, index));
                            }
                        },

                        VecDiff::Clear {} => {
                            Self::remove_existing_futures(futures, &mut new_pending, cx);
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
                if state.future.as_mut().poll(cx).is_ready() {
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
    use std::ops::{Deref, Index, Range, RangeBounds, Bound};
    use std::slice::SliceIndex;
    use std::vec::Drain;
    use std::borrow::Borrow;
    use std::cmp::{Ord, Ordering};
    use std::hash::{Hash, Hasher};
    use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
    use std::task::{Poll, Context};
    use futures_channel::mpsc;
    use futures_util::stream::StreamExt;


    // TODO replace with std::slice::range after it stabilizes
    fn convert_range<R>(range: R, len: usize) -> Range<usize> where R: RangeBounds<usize> {
        let start = match range.start_bound() {
            Bound::Included(&start) => start,
            Bound::Excluded(start) => {
                start.checked_add(1).unwrap_or_else(|| panic!("attempted to index slice from after maximum usize"))
            }
            Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            Bound::Included(end) => {
                end.checked_add(1).unwrap_or_else(|| panic!("attempted to index slice up to maximum usize"))
            }
            Bound::Excluded(&end) => end,
            Bound::Unbounded => len,
        };

        if start > end {
            panic!("slice index starts at {} but ends at {}", start, end);
        }
        if end > len {
            panic!("range end index {} out of range for slice of length {}", end, len);
        }

        Range { start, end }
    }


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

        // TODO can this be improved ?
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
            if old_index != new_index {
                let value = self.values.remove(old_index);
                self.values.insert(new_index, value);
                self.notify(|| VecDiff::Move { old_index, new_index });
            }
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

        fn remove_range(&mut self, range: Range<usize>, mut len: usize) {
            if range.end > range.start {
                if range.start == 0 && range.end == len {
                    self.notify(|| VecDiff::Clear {});

                } else {
                    // TODO use VecDiff::Batch
                    for index in range.into_iter().rev() {
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

        fn drain<R>(&mut self, range: R) -> Drain<'_, A> where R: RangeBounds<usize> {
            let len = self.values.len();
            let range = convert_range(range, len);
            self.remove_range(range.clone(), len);
            self.values.drain(range)
        }

        fn truncate(&mut self, len: usize) {
            let end = self.values.len();
            let range = Range {
                start: len,
                end: end,
            };
            self.remove_range(range, end);
            self.values.truncate(len)
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

        // TODO use reserve / extend
        fn extend<I>(&mut self, iter: I) where I: IntoIterator<Item = A> {
            for value in iter {
                self.push_clone(value);
            }
        }
    }


    // TODO PartialEq with arrays
    macro_rules! make_shared {
        ($t:ty, $r:ty) => {
            impl<'a, A> $t {
                #[inline]
                pub fn as_slice(&self) -> &[A] {
                    self
                }

                #[inline]
                pub fn capacity(&self) -> usize {
                    self.lock.values.capacity()
                }
            }

            impl<'a, 'b, A, B> PartialEq<&'b [B]> for $t where A: PartialEq<B> {
                #[inline] fn eq(&self, other: &&'b [B]) -> bool { self[..] == other[..] }
                #[inline] fn ne(&self, other: &&'b [B]) -> bool { self[..] != other[..] }
            }

            impl<'a, 'b, A, B> PartialEq<$r> for $t where A: PartialEq<B> {
                #[inline] fn eq(&self, other: &$r) -> bool { self[..] == other[..] }
                #[inline] fn ne(&self, other: &$r) -> bool { self[..] != other[..] }
            }

            impl<'a, A> Eq for $t where A: Eq {}

            impl<'a, A> Borrow<[A]> for $t {
                #[inline]
                fn borrow(&self) -> &[A] {
                    &self[..]
                }
            }

            impl<'a, A> PartialOrd for $t where A: PartialOrd {
                #[inline]
                fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                    PartialOrd::partial_cmp(&**self, &**other)
                }
            }

            impl<'a, A> Ord for $t where A: Ord {
                #[inline]
                fn cmp(&self, other: &Self) -> Ordering {
                    Ord::cmp(&**self, &**other)
                }
            }

            impl<'a, A, I> Index<I> for $t where I: SliceIndex<[A]> {
                type Output = I::Output;

                #[inline]
                fn index(&self, index: I) -> &Self::Output {
                    Index::index(&**self, index)
                }
            }

            impl<'a, A> Deref for $t {
                type Target = [A];

                #[inline]
                fn deref(&self) -> &Self::Target {
                    &self.lock.values
                }
            }

            impl<'a, A> Hash for $t where A: Hash {
                #[inline]
                fn hash<H>(&self, state: &mut H) where H: Hasher {
                    Hash::hash(&**self, state)
                }
            }

            impl<'a, A> AsRef<$t> for $t {
                #[inline]
                fn as_ref(&self) -> &$t {
                    self
                }
            }

            impl<'a, A> AsRef<[A]> for $t {
                #[inline]
                fn as_ref(&self) -> &[A] {
                    self
                }
            }
        };
    }


    #[derive(Debug)]
    pub struct MutableVecLockRef<'a, A> where A: 'a {
        lock: RwLockReadGuard<'a, MutableVecState<A>>,
    }

    make_shared!(MutableVecLockRef<'a, A>, MutableVecLockRef<'b, B>);


    // TODO rotate_left, rotate_right, sort, sort_by, sort_by_cached_key, sort_by_key,
    //      sort_unstable, sort_unstable_by, sort_unstable_by_key, dedup, dedup_by,
    //      dedup_by_key, extend_from_slice, resize, resize_with, splice,
    //      split_off, swap_remove, truncate
    // TODO Extend
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

        pub fn swap(&mut self, a: usize, b: usize) {
            if a < b {
                self.move_from_to(a, b);
                self.move_from_to(b - 1, a);

            } else if a > b {
                self.move_from_to(a, b);
                self.move_from_to(b + 1, a);
            }
        }

        #[inline]
        pub fn retain<F>(&mut self, f: F) where F: FnMut(&A) -> bool {
            self.lock.retain(f)
        }

        // TOOD maybe return a custom wrapper ?
        #[inline]
        pub fn drain<R>(&mut self, range: R) -> Drain<'_, A> where R: RangeBounds<usize> {
            self.lock.drain(range)
        }

        #[inline]
        pub fn truncate(&mut self, len: usize) {
            self.lock.truncate(len)
        }

        pub fn reverse(&mut self) {
            let len = self.len();

            if len > 1 {
                let end = len - 1;
                let mut i = 0;

                while i < end {
                    self.move_from_to(end, i);
                    i += 1;
                }
            }
        }

        #[inline]
        pub fn reserve(&mut self, additional: usize) {
            self.lock.values.reserve(additional)
        }

        #[inline]
        pub fn reserve_exact(&mut self, additional: usize) {
            self.lock.values.reserve_exact(additional)
        }

        #[inline]
        pub fn shrink_to_fit(&mut self) {
            self.lock.values.shrink_to_fit()
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

        pub fn apply_vec_diff(this: &mut Self, diff: VecDiff<A>) {
            match diff {
                VecDiff::Replace { values } => this.replace_cloned(values),
                VecDiff::InsertAt { index, value } => this.insert_cloned(index, value),
                VecDiff::UpdateAt { index, value } => this.set_cloned(index, value),
                VecDiff::RemoveAt { index } => { this.remove(index); },
                VecDiff::Move { old_index, new_index } => this.move_from_to(old_index, new_index),
                VecDiff::Push { value } => this.push_cloned(value),
                VecDiff::Pop {} => { this.pop().unwrap(); },
                VecDiff::Clear {} => this.clear(),
            }
        }
    }

    make_shared!(MutableVecLockMut<'a, A>, MutableVecLockMut<'b, B>);

    // TODO extend_one and extend_reserve
    impl<'a, A> Extend<A> for MutableVecLockMut<'a, A> where A: Clone {
        #[inline]
        fn extend<I>(&mut self, iter: I) where I: IntoIterator<Item = A> {
            self.lock.extend(iter)
        }
    }


    // TODO get rid of the Arc
    // TODO impl some of the same traits as Vec
    pub struct MutableVec<A>(Arc<RwLock<MutableVecState<A>>>);

    impl<A> MutableVec<A> {
        // TODO deprecate this and replace with with_values ?
        #[inline]
        pub fn new_with_values(values: Vec<A>) -> Self {
            Self::from(values)
        }

        #[inline]
        pub fn new() -> Self {
            Self::new_with_values(vec![])
        }

        #[inline]
        pub fn with_capacity(capacity: usize) -> Self {
            Self::new_with_values(Vec::with_capacity(capacity))
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

    impl<T, A> From<T> for MutableVec<A> where Vec<A>: From<T> {
        #[inline]
        fn from(values: T) -> Self {
            MutableVec(Arc::new(RwLock::new(MutableVecState {
                values: values.into(),
                senders: vec![],
            })))
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

    #[cfg(feature = "serde")]
    impl<T> serde::Serialize for MutableVec<T> where T: serde::Serialize {
        #[inline]
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: serde::Serializer {
            self.0.read().unwrap().values.serialize(serializer)
        }
    }

    #[cfg(feature = "serde")]
    impl<'de, T> serde::Deserialize<'de> for MutableVec<T> where T: serde::Deserialize<'de> {
        #[inline]
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: serde::Deserializer<'de> {
            <Vec<T>>::deserialize(deserializer).map(MutableVec::new_with_values)
        }
    }

    impl<T> Default for MutableVec<T> {
        #[inline]
        fn default() -> Self {
            MutableVec::new()
        }
    }

    impl<T> Clone for MutableVec<T> {
        #[inline]
        fn clone(&self) -> Self {
            MutableVec(self.0.clone())
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
        fn poll_vec_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
            self.receiver.poll_next_unpin(cx)
        }
    }
}

pub use self::mutable_vec::*;
