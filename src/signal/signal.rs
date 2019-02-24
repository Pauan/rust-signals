use internal::Map2;
use std::pin::Pin;
use std::marker::Unpin;
use futures_core::task::Waker;
use futures_core::Poll;
use futures_core::future::Future;
use futures_core::stream::Stream;
use futures_util::stream;
use futures_util::stream::StreamExt;
use signal_vec::{VecDiff, SignalVec};


// TODO impl for AssertUnwindSafe ?
pub trait Signal {
    type Item;

    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>>;
}


// Copied from Future in the Rust stdlib
impl<'a, A> Signal for &'a mut A where A: ?Sized + Signal + Unpin {
    type Item = A::Item;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        A::poll_change(Pin::new(&mut **self), waker)
    }
}

// Copied from Future in the Rust stdlib
impl<A> Signal for Box<A> where A: ?Sized + Signal + Unpin {
    type Item = A::Item;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        A::poll_change(Pin::new(&mut *self), waker)
    }
}

// Copied from Future in the Rust stdlib
impl<A> Signal for Pin<A>
    where A: Unpin + ::std::ops::DerefMut,
          A::Target: Signal {
    type Item = <<A as ::std::ops::Deref>::Target as Signal>::Item;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        Pin::get_mut(self).as_mut().poll_change(waker)
    }
}


pub trait SignalExt: Signal {
    /// Creates a `Stream` which contains the values of `self`.
    ///
    /// When the output `Stream` is spawned:
    ///
    /// 1. It immediately outputs the current value of `self`.
    ///
    /// 2. Whenever `self` changes it outputs the new value of `self`.
    ///
    /// Like *all* of the `Signal` methods, `to_stream` might skip intermediate changes.
    /// So you ***cannot*** rely upon it containing every intermediate change.
    /// But you ***can*** rely upon it always containing the most recent change.
    ///
    /// # Performance
    ///
    /// This is ***extremely*** efficient: it is *guaranteed* constant time, and it does not do
    /// any heap allocation.
    #[inline]
    fn to_stream(self) -> SignalStream<Self>
        where Self: Sized {
        SignalStream {
            signal: self,
        }
    }

    // TODO maybe remove this ?
    #[inline]
    fn to_future(self) -> SignalFuture<Self>
        where Self: Sized {
        SignalFuture {
            signal: self,
            value: None,
        }
    }

    /// Creates a `Signal` which uses a closure to transform the value.
    ///
    /// When the output `Signal` is spawned:
    ///
    /// 1. It calls the closure with the current value of `self`.
    ///
    /// 2. Then it puts the return value of the closure into the output `Signal`.
    ///
    /// 3. Whenever `self` changes it repeats the above steps.
    ///
    ///    This happens automatically and efficiently.
    ///
    /// It will call the closure at most once for each change in `self`.
    ///
    /// Like *all* of the `Signal` methods, `map` might skip intermediate changes.
    /// So you ***cannot*** rely upon the closure being called for every intermediate change.
    /// But you ***can*** rely upon it always being called with the most recent change.
    ///
    /// # Examples
    ///
    /// Add `1` to the value:
    ///
    /// ```rust
    /// # use futures_signals::signal::{always, SignalExt};
    /// # let input = always(1);
    /// let mapped = input.map(|value| value + 1);
    /// ```
    ///
    /// `mapped` will always contain the current value of `input`, except with `1` added to it.
    ///
    /// If `input` has the value `10`, then `mapped` will have the value `11`.
    ///
    /// If `input` has the value `5`, then `mapped` will have the value `6`, etc.
    ///
    /// ----
    ///
    /// Formatting to a `String`:
    ///
    /// ```rust
    /// # use futures_signals::signal::{always, SignalExt};
    /// # let input = always(1);
    /// let mapped = input.map(|value| format!("{}", value));
    /// ```
    ///
    /// `mapped` will always contain the current value of `input`, except formatted as a `String`.
    ///
    /// If `input` has the value `10`, then `mapped` will have the value `"10"`.
    ///
    /// If `input` has the value `5`, then `mapped` will have the value `"5"`, etc.
    ///
    /// # Performance
    ///
    /// This is ***extremely*** efficient: it is *guaranteed* constant time, and it does not do
    /// any heap allocation.
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
    fn inspect<A>(self, callback: A) -> Inspect<Self, A>
        where A: FnMut(&Self::Item),
              Self: Sized {
        Inspect {
            signal: self,
            callback,
        }
    }

    /// Creates a `Signal` which uses a closure to transform the value.
    ///
    /// This is exactly the same as `map`, except:
    ///
    /// 1. It calls the closure with a mutable reference to the input value.
    ///
    /// 2. If the new input value is the same as the old input value, it will ***not*** call the closure, instead
    ///    it will completely ignore the new value, like as if it never happened.
    ///
    ///    It uses the `PartialEq` implementation to determine whether the new value is the same as the old value.
    ///
    ///    It only keeps track of the most recent value: that means that it ***won't*** call the closure for consecutive
    ///    duplicates, however it ***will*** call the closure for non-consecutive duplicates.
    ///
    /// Because `dedupe_map` has the same behavior as `map`, it is useful solely as a performance optimization.
    ///
    /// # Performance
    ///
    /// The performance is the same as `map`, except with an additional call to `eq`.
    ///
    /// If the `eq` call is fast, then `dedupe_map` can be faster than `map`, because it doesn't call the closure
    /// when the new and old values are the same, and it also doesn't update any child Signals.
    ///
    /// On the other hand, if the `eq` call is slow, then `dedupe_map` is probably slower than `map`.
    #[inline]
    fn dedupe_map<A, B>(self, callback: B) -> DedupeMap<Self, B>
        // TODO should this use & instead of &mut ?
        where B: FnMut(&mut Self::Item) -> A,
              Self: Sized {
        DedupeMap {
            old_value: None,
            signal: self,
            callback,
        }
    }

    #[inline]
    fn dedupe(self) -> Dedupe<Self> where Self: Sized {
        Dedupe {
            old_value: None,
            signal: self,
        }
    }

    #[inline]
    fn dedupe_cloned(self) -> DedupeCloned<Self> where Self: Sized {
        DedupeCloned {
            old_value: None,
            signal: self,
        }
    }

    /// Creates a `Signal` which uses a closure to asynchronously transform the value.
    ///
    /// When the output `Signal` is spawned:
    ///
    /// 1. It calls the closure with the current value of `self`.
    ///
    /// 2. The closure returns a `Future`. It waits for that `Future` to finish, and then
    ///    it puts the return value of the `Future` into the output `Signal`.
    ///
    /// 3. Whenever `self` changes it repeats the above steps.
    ///
    /// It will call the closure at most once for each change in `self`.
    ///
    /// Because Signals must always have a current value, if the `Future` is not ready yet, then the
    /// output `Signal` will start with the value `None`. When the `Future` finishes it then changes
    /// to `Some`. This can be used to detect whether the `Future` has finished or not.
    ///
    /// If `self` changes before the old `Future` is finished, it will cancel the old `Future`.
    /// That means if `self` changes faster than the `Future`, then it will never output any values.
    ///
    /// Like *all* of the `Signal` methods, `map_future` might skip intermediate changes.
    /// So you ***cannot*** rely upon the closure being called for every intermediate change.
    /// But you ***can*** rely upon it always being called with the most recent change.
    ///
    /// # Examples
    ///
    /// Call an asynchronous network API whenever the input changes:
    ///
    /// ```rust
    /// # extern crate futures_core;
    /// # extern crate futures_util;
    /// # extern crate futures_signals;
    /// # use futures_signals::signal::{always, SignalExt};
    /// # use futures_util::future::{ready, Ready};
    /// # fn call_network_api(value: u32) -> Ready<()> { ready(()) }
    /// # fn main() {
    /// # let input = always(1);
    /// #
    /// let mapped = input.map_future(|value| call_network_api(value));
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// This is ***extremely*** efficient: it does not do any heap allocation, and it has *very* little overhead.
    ///
    /// Of course the performance will also depend upon the `Future` which is returned from the closure.
    #[inline]
    fn map_future<A, B>(self, callback: B) -> MapFuture<Self, A, B>
        where A: Future,
              B: FnMut(Self::Item) -> A,
              Self: Sized {
        MapFuture {
            signal: Some(self),
            future: None,
            callback,
            first: true,
        }
    }

    /// Creates a `Signal` which uses a closure to filter and transform the value.
    ///
    /// When the output `Signal` is spawned:
    ///
    /// 1. The output `Signal` starts with the value `None`.
    ///
    /// 2. It calls the closure with the current value of `self`.
    ///
    /// 3. If the closure returns `Some`, then it puts the return value of the closure into the output `Signal`.
    ///
    /// 4. If the closure returns `None`, then it does nothing.
    ///
    /// 5. Whenever `self` changes it repeats steps 2 - 4.
    ///
    /// The output `Signal` will only be `None` for the initial value. After that it will always be `Some`.
    ///
    /// If the closure returns `Some` for the initial value, then the output `Signal` will never be `None`.
    ///
    /// It will call the closure at most once for each change in `self`.
    ///
    /// Like *all* of the `Signal` methods, `filter_map` might skip intermediate changes.
    /// So you ***cannot*** rely upon the closure being called for every intermediate change.
    /// But you ***can*** rely upon it always being called with the most recent change.
    ///
    /// # Examples
    ///
    /// Add `1` to the value, but only if the value is less than `5`:
    ///
    /// ```rust
    /// # use futures_signals::signal::{always, SignalExt};
    /// # let input = always(1);
    /// let mapped = input.filter_map(|value| {
    ///     if value < 5 {
    ///         Some(value + 1)
    ///
    ///     } else {
    ///         None
    ///     }
    /// });
    /// ```
    ///
    /// If the initial value of `input` is `5` or greater then `mapped` will be `None`.
    ///
    /// If the current value of `input` is `5` or greater then `mapped` will keep its old value.
    ///
    /// Otherwise `mapped` will be `Some(input + 1)`.
    ///
    /// # Performance
    ///
    /// This is ***extremely*** efficient: it does not do any heap allocation, and it has *very* little overhead.
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

    /// Creates a `Signal` which flattens `self`.
    ///
    /// When the output `Signal` is spawned:
    ///
    /// 1. It retrieves the current value of `self` (this value is also a `Signal`).
    ///
    /// 2. Then it puts the current value of the inner `Signal` into the output `Signal`.
    ///
    /// 3. Whenever the inner `Signal` changes it puts the new value into the output `Signal`.
    ///
    /// 4. Whenever `self` changes it repeats the above steps.
    ///
    ///    This happens automatically and efficiently.
    ///
    /// Like *all* of the `Signal` methods, `flatten` might skip intermediate changes.
    /// So you ***cannot*** rely upon it containing every intermediate change.
    /// But you ***can*** rely upon it always containing the most recent change.
    ///
    /// # Performance
    ///
    /// This is very efficient: it is *guaranteed* constant time, and it does not do
    /// any heap allocation.
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
    fn switch<A, B>(self, callback: B) -> Switch<Self, A, B>
        where A: Signal,
              B: FnMut(Self::Item) -> A,
              Self: Sized {
        Switch {
            inner: self.map(callback).flatten()
        }
    }

    #[inline]
    // TODO file Rust bug about bad error message when `callback` isn't marked as `mut`
    fn for_each<U, F>(self, callback: F) -> ForEach<Self, U, F>
        where U: Future<Output = ()>,
              F: FnMut(Self::Item) -> U,
              Self: Sized {
        // TODO a bit hacky
        ForEach {
            inner: SignalStream {
                signal: self,
            }.for_each(callback)
        }
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
    fn first(self) -> First<Self> where Self: Sized {
        First {
            signal: Some(self),
        }
    }

    /// A convenience for calling `Signal::poll_change` on `Unpin` types.
    #[inline]
    fn poll_change_unpin(&mut self, waker: &Waker) -> Poll<Option<Self::Item>> where Self: Unpin + Sized {
        Pin::new(self).poll_change(waker)
    }
}

// TODO why is this ?Sized
impl<T: ?Sized> SignalExt for T where T: Signal {}


// TODO make this into a method later
#[inline]
pub fn not<A>(signal: A) -> impl Signal<Item = bool>
    where A: Signal<Item = bool> {
    signal.map(|x| !x)
}

// TODO make this into a method later
// TODO use short-circuiting if the left signal returns false ?
#[inline]
pub fn and<A, B>(left: A, right: B) -> impl Signal<Item = bool>
    where A: Signal<Item = bool>,
          B: Signal<Item = bool> {
    Map2::new(left, right, |a, b| *a && *b)
}

// TODO make this into a method later
// TODO use short-circuiting if the left signal returns true ?
#[inline]
pub fn or<A, B>(left: A, right: B) -> impl Signal<Item = bool>
    where A: Signal<Item = bool>,
          B: Signal<Item = bool> {
    Map2::new(left, right, |a, b| *a || *b)
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct FromFuture<A> {
    // TODO is this valid with pinned types ?
    future: Option<A>,
    first: bool,
}

impl<A> Unpin for FromFuture<A> where A: Unpin {}

impl<A> Signal for FromFuture<A> where A: Future {
    type Item = Option<A::Output>;

    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin future,
            mut first,
        });

        // TODO is this valid with pinned types ?
        match future.as_mut().as_pin_mut().map(|future| future.poll(waker)) {
            None => {
                Poll::Ready(None)
            },

            Some(Poll::Ready(value)) => {
                future.set(None);
                Poll::Ready(Some(Some(value)))
            },

            Some(Poll::Pending) => {
                if *first {
                    *first = false;
                    Poll::Ready(Some(None))

                } else {
                    Poll::Pending
                }
            },
        }
    }
}

#[inline]
pub fn from_future<A>(future: A) -> FromFuture<A> where A: Future {
    FromFuture { future: Some(future), first: true }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct FromStream<A> {
    stream: A,
    first: bool,
}

impl<A> Unpin for FromStream<A> where A: Unpin {}

impl<A> Signal for FromStream<A> where A: Stream {
    type Item = Option<A::Item>;

    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin stream,
            mut first,
        });

        match stream.poll_next(waker) {
            Poll::Ready(None) => {
                Poll::Ready(None)
            },

            Poll::Ready(Some(value)) => {
                Poll::Ready(Some(Some(value)))
            },

            Poll::Pending => {
                if *first {
                    *first = false;
                    Poll::Ready(Some(None))

                } else {
                    Poll::Pending
                }
            },
        }
    }
}

#[inline]
pub fn from_stream<A>(stream: A) -> FromStream<A> where A: Stream {
    FromStream { stream, first: true }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Always<A> {
    value: Option<A>,
}

impl<A> Unpin for Always<A> {}

impl<A> Signal for Always<A> {
    type Item = A;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, _: &Waker) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.value.take())
    }
}

#[inline]
pub fn always<A>(value: A) -> Always<A> {
    Always {
        value: Some(value),
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct First<A> {
    signal: Option<A>,
}

impl<A> Unpin for First<A> where A: Unpin {}

impl<A> Signal for First<A> where A: Signal {
    type Item = A::Item;

    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin signal,
        });

        // TODO maybe it's safe to replace this with take ?
        if let Some(poll) = signal.as_mut().as_pin_mut().map(|signal| signal.poll_change(waker)) {
            signal.set(None);
            poll

        } else {
            Poll::Ready(None)
        }
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Switch<A, B, C> where A: Signal, C: FnMut(A::Item) -> B {
    inner: Flatten<Map<A, C>>,
}

// TODO is this correct ?
impl<A, B, C> Unpin for Switch<A, B, C> where A: Signal + Unpin, B: Unpin, C: FnMut(A::Item) -> B {}

impl<A, B, C> Signal for Switch<A, B, C>
    where A: Signal,
          B: Signal,
          C: FnMut(A::Item) -> B {
    type Item = B::Item;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin inner,
        });

        inner.poll_change(waker)
    }
}


#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct ForEach<A, B, C> {
    inner: stream::ForEach<SignalStream<A>, B, C>,
}

impl<A, B, C> Unpin for ForEach<A, B, C> where A: Signal + Unpin, B: Future<Output = ()> + Unpin, C: FnMut(A::Item) -> B {}

impl<A, B, C> Future for ForEach<A, B, C>
    where A: Signal,
          B: Future<Output = ()>,
          C: FnMut(A::Item) -> B {
    type Output = ();

    #[inline]
    fn poll(self: Pin<&mut Self>, waker: &Waker) -> Poll<Self::Output> {
        unsafe_project!(self => {
            pin inner,
        });

        inner.poll(waker)
    }
}


#[derive(Debug)]
#[must_use = "Streams do nothing unless polled"]
pub struct SignalStream<A> {
    signal: A,
}

impl<A> Unpin for SignalStream<A> where A: Unpin {}

impl<A: Signal> Stream for SignalStream<A> {
    type Item = A::Item;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin signal,
        });

        signal.poll_change(waker)
    }
}


// TODO maybe remove this ?
#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct SignalFuture<A> where A: Signal {
    signal: A,
    value: Option<A::Item>,
}

impl<A> Unpin for SignalFuture<A> where A: Unpin + Signal {}

impl<A> Future for SignalFuture<A> where A: Signal {
    type Output = A::Item;

    #[inline]
    fn poll(self: Pin<&mut Self>, waker: &Waker) -> Poll<Self::Output> {
        unsafe_project!(self => {
            pin signal,
            mut value,
        });

        loop {
            return match signal.as_mut().poll_change(waker) {
                Poll::Ready(None) => {
                    Poll::Ready(value.take().unwrap())
                },
                Poll::Ready(Some(new_value)) => {
                    *value = Some(new_value);
                    continue;
                },
                Poll::Pending => {
                    Poll::Pending
                },
            }
        }
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Map<A, B> {
    signal: A,
    callback: B,
}

impl<A, B> Unpin for Map<A, B> where A: Unpin {}

impl<A, B, C> Signal for Map<A, B>
    where A: Signal,
          B: FnMut(A::Item) -> C {
    type Item = C;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin signal,
            mut callback,
        });

        signal.poll_change(waker).map(|opt| opt.map(|value| callback(value)))
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Inspect<A, B> {
    signal: A,
    callback: B,
}

impl<A, B> Unpin for Inspect<A, B> where A: Unpin {}

impl<A, B> Signal for Inspect<A, B>
    where A: Signal,
          B: FnMut(&A::Item) {
    type Item = A::Item;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin signal,
            mut callback,
        });

        let poll = signal.poll_change(waker);

        if let Poll::Ready(Some(ref value)) = poll {
            callback(value);
        }

        poll
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct MapFuture<A, B, C> {
    signal: Option<A>,
    future: Option<B>,
    callback: C,
    first: bool,
}

impl<A, B, C> Unpin for MapFuture<A, B, C> where A: Unpin, B: Unpin {}

impl<A, B, C> Signal for MapFuture<A, B, C>
    where A: Signal,
          B: Future,
          C: FnMut(A::Item) -> B {
    type Item = Option<B::Output>;

    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin signal,
            pin future,
            mut callback,
            mut first,
        });

        let mut done = false;

        loop {
            match signal.as_mut().as_pin_mut().map(|signal| signal.poll_change(waker)) {
                None => {
                    done = true;
                },
                Some(Poll::Ready(None)) => {
                    signal.set(None);
                    done = true;
                },
                Some(Poll::Ready(Some(value))) => {
                    let value = Some(callback(value));
                    future.set(value);
                    continue;
                },
                Some(Poll::Pending) => {},
            }
            break;
        }

        match future.as_mut().as_pin_mut().map(|future| future.poll(waker)) {
            None => {},
            Some(Poll::Ready(value)) => {
                future.set(None);
                *first = false;
                return Poll::Ready(Some(Some(value)));
            },
            Some(Poll::Pending) => {
                done = false;
            },
        }

        if *first {
            *first = false;
            Poll::Ready(Some(None))

        } else if done {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}


#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct WaitFor<A>
    where A: Signal,
          A::Item: PartialEq {
    signal: A,
    value: A::Item,
}

impl<A> Unpin for WaitFor<A> where A: Unpin + Signal, A::Item: PartialEq {}

impl<A> Future for WaitFor<A>
    where A: Signal,
          A::Item: PartialEq {

    type Output = Option<A::Item>;

    fn poll(self: Pin<&mut Self>, waker: &Waker) -> Poll<Self::Output> {
        unsafe_project!(self => {
            pin signal,
            mut value,
        });

        loop {
            let poll = signal.as_mut().poll_change(waker);

            if let Poll::Ready(Some(ref new_value)) = poll {
                if new_value != value {
                    continue;
                }
            }

            return poll;
        }
    }
}


#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct SignalSignalVec<A> {
    signal: A,
}

impl<A> Unpin for SignalSignalVec<A> where A: Unpin {}

impl<A, B> SignalVec for SignalSignalVec<A>
    where A: Signal<Item = Vec<B>> {
    type Item = B;

    #[inline]
    fn poll_vec_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<VecDiff<Self::Item>>> {
        unsafe_project!(self => {
            pin signal,
        });

        signal.poll_change(waker).map(|opt| opt.map(|values| VecDiff::Replace { values }))
    }
}


macro_rules! dedupe {
    ($signal:expr, $waker:expr, $value:expr, $pat:pat, $name:ident => $output:expr) => {
        loop {
            return match $signal.as_mut().poll_change($waker) {
                Poll::Ready(Some($pat)) => {
                    let has_changed = match $value {
                        Some(ref old_value) => *old_value != $name,
                        None => true,
                    };

                    if has_changed {
                        let output = $output;
                        *$value = Some($name);
                        Poll::Ready(Some(output))

                    } else {
                        continue;
                    }
                },
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            }
        }
    };
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct DedupeMap<A, B> where A: Signal {
    old_value: Option<A::Item>,
    signal: A,
    callback: B,
}

impl<A, B> Unpin for DedupeMap<A, B> where A: Unpin + Signal {}

impl<A, B, C> Signal for DedupeMap<A, B>
    where A: Signal,
          A::Item: PartialEq,
          // TODO should this use & instead of &mut ?
          // TODO should this use Fn instead ?
          B: FnMut(&mut A::Item) -> C {

    type Item = C;

    // TODO should this use #[inline] ?
    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            mut old_value,
            pin signal,
            mut callback,
        });

        dedupe!(signal, waker, old_value, mut value, value => callback(&mut value))
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Dedupe<A> where A: Signal {
    old_value: Option<A::Item>,
    signal: A,
}

impl<A> Unpin for Dedupe<A> where A: Unpin + Signal {}

impl<A> Signal for Dedupe<A>
    where A: Signal,
          A::Item: PartialEq + Copy {

    type Item = A::Item;

    // TODO should this use #[inline] ?
    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            mut old_value,
            pin signal,
        });

        dedupe!(signal, waker, old_value, value, value => value)
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct DedupeCloned<A> where A: Signal {
    old_value: Option<A::Item>,
    signal: A,
}

impl<A> Unpin for DedupeCloned<A> where A: Unpin + Signal {}

impl<A> Signal for DedupeCloned<A>
    where A: Signal,
          A::Item: PartialEq + Clone {

    type Item = A::Item;

    // TODO should this use #[inline] ?
    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            mut old_value,
            pin signal,
        });

        dedupe!(signal, waker, old_value, value, value => value.clone())
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct FilterMap<A, B> {
    signal: A,
    callback: B,
    first: bool,
}

impl<A, B> Unpin for FilterMap<A, B> where A: Unpin {}

impl<A, B, C> Signal for FilterMap<A, B>
    where A: Signal,
          B: FnMut(A::Item) -> Option<C> {
    type Item = Option<C>;

    // TODO should this use #[inline] ?
    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin signal,
            mut callback,
            mut first,
        });

        loop {
            return match signal.as_mut().poll_change(waker) {
                Poll::Ready(Some(value)) => match callback(value) {
                    Some(value) => {
                        *first = false;
                        Poll::Ready(Some(Some(value)))
                    },

                    None => {
                        if *first {
                            *first = false;
                            Poll::Ready(Some(None))

                        } else {
                            continue;
                        }
                    },
                },
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Flatten<A> where A: Signal {
    signal: Option<A>,
    inner: Option<A::Item>,
}

// TODO is this impl correct ?
impl<A> Unpin for Flatten<A> where A: Unpin + Signal, A::Item: Unpin {}

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
    type Item = <A::Item as Signal>::Item;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, waker: &Waker) -> Poll<Option<Self::Item>> {
        unsafe_project!(self => {
            pin signal,
            pin inner,
        });

        let done = match signal.as_mut().as_pin_mut().map(|signal| signal.poll_change(waker)) {
            None => true,
            Some(Poll::Ready(None)) => {
                signal.set(None);
                true
            },
            Some(Poll::Ready(Some(new_inner))) => {
                inner.set(Some(new_inner));
                false
            },
            Some(Poll::Pending) => false,
        };

        match inner.as_mut().as_pin_mut().map(|inner| inner.poll_change(waker)) {
            Some(Poll::Ready(None)) => {
                inner.set(None);
            },
            Some(poll) => {
                return poll;
            },
            None => {},
        }

        if done {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}
