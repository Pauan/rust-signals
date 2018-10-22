use std::pin::{Pin, Unpin};
use futures_core::task::LocalWaker;
use futures_core::Poll;
use futures_core::future::Future;
use futures_core::stream::Stream;
use futures_util::stream;
use futures_util::stream::StreamExt;
use signal_vec::{VecDiff, SignalVec};
use pin_utils::{unsafe_pinned, unsafe_unpinned};


// TODO impl for AssertUnwindSafe and Pin ?
pub trait Signal {
    type Item;

    fn poll_change(self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>>;
}


// Copied from Future in the Rust stdlib
impl<'a, A: ?Sized + Signal + Unpin> Signal for &'a mut A {
    type Item = A::Item;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        A::poll_change(Pin::new(&mut **self), waker)
    }
}

// Copied from Future in the Rust stdlib
impl<A: ?Sized + Signal + Unpin> Signal for Box<A> {
    type Item = A::Item;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        A::poll_change(Pin::new(&mut *self), waker)
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
    fn poll_change_unpin(&mut self, waker: &LocalWaker) -> Poll<Option<Self::Item>> where Self: Unpin + Sized {
        Pin::new(self).poll_change(waker)
    }
}

// TODO why is this ?Sized
impl<T: ?Sized> SignalExt for T where T: Signal {}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct FromFuture<A> {
    // TODO is this valid with pinned types ?
    future: Option<A>,
    first: bool,
}

impl<A> FromFuture<A> where A: Future {
    unsafe_pinned!(future: Option<A>);
    unsafe_unpinned!(first: bool);
}

impl<A> Unpin for FromFuture<A> where A: Unpin {}

impl<A> Signal for FromFuture<A> where A: Future {
    type Item = Option<A::Output>;

    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        // TODO is this valid with pinned types ?
        match self.future().as_pin_mut().map(|future| future.poll(waker)) {
            None => {
                Poll::Ready(None)
            },

            Some(Poll::Ready(value)) => {
                Pin::set(self.future(), None);
                Poll::Ready(Some(Some(value)))
            },

            Some(Poll::Pending) => {
                let first = self.first();

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

impl<A> FromStream<A> {
    unsafe_pinned!(stream: A);
    unsafe_unpinned!(first: bool);
}

impl<A> Unpin for FromStream<A> where A: Unpin {}

impl<A> Signal for FromStream<A> where A: Stream {
    type Item = Option<A::Item>;

    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        match self.stream().poll_next(waker) {
            Poll::Ready(None) => {
                Poll::Ready(None)
            },

            Poll::Ready(Some(value)) => {
                Poll::Ready(Some(Some(value)))
            },

            Poll::Pending => {
                let first = self.first();

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
    fn poll_change(mut self: Pin<&mut Self>, _: &LocalWaker) -> Poll<Option<Self::Item>> {
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

impl<A> First<A> {
    unsafe_pinned!(signal: Option<A>);
}

impl<A> Unpin for First<A> where A: Unpin {}

impl<A> Signal for First<A> where A: Signal {
    type Item = A::Item;

    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        // TODO maybe it's safe to replace this with take ?
        if let Some(poll) = self.signal().as_pin_mut().map(|signal| signal.poll_change(waker)) {
            Pin::set(self.signal(), None);
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

impl<A, B, C> Switch<A, B, C> where A: Signal, C: FnMut(A::Item) -> B {
    unsafe_pinned!(inner: Flatten<Map<A, C>>);
}

// TODO is this correct ?
impl<A, B, C> Unpin for Switch<A, B, C> where A: Signal + Unpin, B: Unpin, C: FnMut(A::Item) -> B {}

impl<A, B, C> Signal for Switch<A, B, C>
    where A: Signal,
          B: Signal,
          C: FnMut(A::Item) -> B {
    type Item = B::Item;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        self.inner().poll_change(waker)
    }
}


#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct ForEach<A, B, C> {
    inner: stream::ForEach<SignalStream<A>, B, C>,
}

impl<A, B, C> ForEach<A, B, C> {
    unsafe_pinned!(inner: stream::ForEach<SignalStream<A>, B, C>);
}

impl<A, B, C> Unpin for ForEach<A, B, C> where A: Signal + Unpin, B: Future<Output = ()> + Unpin, C: FnMut(A::Item) -> B {}

impl<A, B, C> Future for ForEach<A, B, C>
    where A: Signal,
          B: Future<Output = ()>,
          C: FnMut(A::Item) -> B {
    type Output = ();

    #[inline]
    fn poll(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Self::Output> {
        self.inner().poll(waker)
    }
}


#[derive(Debug)]
#[must_use = "Streams do nothing unless polled"]
pub struct SignalStream<A> {
    signal: A,
}

impl<A> SignalStream<A> {
    unsafe_pinned!(signal: A);
}

impl<A> Unpin for SignalStream<A> where A: Unpin {}

impl<A: Signal> Stream for SignalStream<A> {
    type Item = A::Item;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        self.signal().poll_change(waker)
    }
}


// TODO maybe remove this ?
#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct SignalFuture<A> where A: Signal {
    signal: A,
    value: Option<A::Item>,
}

impl<A> SignalFuture<A> where A: Signal {
    unsafe_pinned!(signal: A);
    unsafe_unpinned!(value: Option<A::Item>);
}

impl<A> Unpin for SignalFuture<A> where A: Unpin + Signal {}

impl<A> Future for SignalFuture<A> where A: Signal {
    type Output = A::Item;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Self::Output> {
        loop {
            return match self.signal().poll_change(waker) {
                Poll::Ready(None) => {
                    Poll::Ready(self.value().take().unwrap())
                },
                Poll::Ready(Some(value)) => {
                    *self.value() = Some(value);
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

impl<A, B> Map<A, B> {
    unsafe_pinned!(signal: A);
    unsafe_unpinned!(callback: B);
}

impl<A, B> Unpin for Map<A, B> where A: Unpin {}

impl<A, B, C> Signal for Map<A, B>
    where A: Signal,
          B: FnMut(A::Item) -> C {
    type Item = C;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        self.signal().poll_change(waker).map(|opt| opt.map(|value| self.callback()(value)))
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

impl<A, B, C> MapFuture<A, B, C> {
    unsafe_pinned!(signal: Option<A>);
    unsafe_pinned!(future: Option<B>);
    unsafe_unpinned!(callback: C);
    unsafe_unpinned!(first: bool);
}

impl<A, B, C> Unpin for MapFuture<A, B, C> where A: Unpin, B: Unpin {}

impl<A, B, C> Signal for MapFuture<A, B, C>
    where A: Signal,
          B: Future,
          C: FnMut(A::Item) -> B {
    type Item = Option<B::Output>;

    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        let mut done = false;

        loop {
            match self.signal().as_pin_mut().map(|signal| signal.poll_change(waker)) {
                None => {
                    done = true;
                },
                Some(Poll::Ready(None)) => {
                    Pin::set(self.signal(), None);
                    done = true;
                },
                Some(Poll::Ready(Some(value))) => {
                    let value = Some(self.callback()(value));
                    Pin::set(self.future(), value);
                    continue;
                },
                Some(Poll::Pending) => {},
            }
            break;
        }

        match self.future().as_pin_mut().map(|future| future.poll(waker)) {
            None => {},
            Some(Poll::Ready(value)) => {
                Pin::set(self.future(), None);
                *self.first() = false;
                return Poll::Ready(Some(Some(value)));
            },
            Some(Poll::Pending) => {
                done = false;
            },
        }

        let first = self.first();

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

impl<A> WaitFor<A> where A: Signal, A::Item: PartialEq {
    unsafe_pinned!(signal: A);
}

impl<A> Unpin for WaitFor<A> where A: Unpin + Signal, A::Item: PartialEq {}

impl<A> Future for WaitFor<A>
    where A: Signal,
          A::Item: PartialEq {

    type Output = Option<A::Item>;

    fn poll(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Self::Output> {
        loop {
            let poll = self.signal().poll_change(waker);

            if let Poll::Ready(Some(ref value)) = poll {
                if *value != self.value {
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

impl<A> SignalSignalVec<A> {
    unsafe_pinned!(signal: A);
}

impl<A> Unpin for SignalSignalVec<A> where A: Unpin {}

impl<A, B> SignalVec for SignalSignalVec<A>
    where A: Signal<Item = Vec<B>> {
    type Item = B;

    #[inline]
    fn poll_vec_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<VecDiff<Self::Item>>> {
        self.signal().poll_change(waker).map(|opt| opt.map(|values| VecDiff::Replace { values }))
    }
}


macro_rules! dedupe {
    ($signal:expr, $waker:expr, $value:expr, $pat:pat, $name:ident => $output:expr) => {
        loop {
            return match $signal.poll_change($waker) {
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

impl<A, B> DedupeMap<A, B> where A: Signal {
    unsafe_unpinned!(old_value: Option<A::Item>);
    unsafe_pinned!(signal: A);
    unsafe_unpinned!(callback: B);
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
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        dedupe!(self.signal(), waker, self.old_value(), mut value, value => self.callback()(&mut value))
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Dedupe<A> where A: Signal {
    old_value: Option<A::Item>,
    signal: A,
}

impl<A> Dedupe<A> where A: Signal {
    unsafe_unpinned!(old_value: Option<A::Item>);
    unsafe_pinned!(signal: A);
}

impl<A> Unpin for Dedupe<A> where A: Unpin + Signal {}

impl<A> Signal for Dedupe<A>
    where A: Signal,
          A::Item: PartialEq + Copy {

    type Item = A::Item;

    // TODO should this use #[inline] ?
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        dedupe!(self.signal(), waker, self.old_value(), value, value => value)
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct DedupeCloned<A> where A: Signal {
    old_value: Option<A::Item>,
    signal: A,
}

impl<A> DedupeCloned<A> where A: Signal {
    unsafe_unpinned!(old_value: Option<A::Item>);
    unsafe_pinned!(signal: A);
}

impl<A> Unpin for DedupeCloned<A> where A: Unpin + Signal {}

impl<A> Signal for DedupeCloned<A>
    where A: Signal,
          A::Item: PartialEq + Clone {

    type Item = A::Item;

    // TODO should this use #[inline] ?
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        dedupe!(self.signal(), waker, self.old_value(), value, value => value.clone())
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct FilterMap<A, B> {
    signal: A,
    callback: B,
    first: bool,
}

impl<A, B> FilterMap<A, B> {
    unsafe_pinned!(signal: A);
    unsafe_unpinned!(callback: B);
    unsafe_unpinned!(first: bool);
}

impl<A, B> Unpin for FilterMap<A, B> where A: Unpin {}

impl<A, B, C> Signal for FilterMap<A, B>
    where A: Signal,
          B: FnMut(A::Item) -> Option<C> {
    type Item = Option<C>;

    // TODO should this use #[inline] ?
    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        loop {
            return match self.signal().poll_change(waker) {
                Poll::Ready(Some(value)) => match self.callback()(value) {
                    Some(value) => {
                        *self.first() = false;
                        Poll::Ready(Some(Some(value)))
                    },

                    None => {
                        let first = self.first();

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

impl<A> Flatten<A> where A: Signal {
    unsafe_pinned!(signal: Option<A>);
    unsafe_pinned!(inner: Option<A::Item>);
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
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        let done = match self.signal().as_pin_mut().map(|signal| signal.poll_change(waker)) {
            None => true,
            Some(Poll::Ready(None)) => {
                Pin::set(self.signal(), None);
                true
            },
            Some(Poll::Ready(Some(inner))) => {
                Pin::set(self.inner(), Some(inner));
                false
            },
            Some(Poll::Pending) => false,
        };

        match self.inner().as_pin_mut().map(|inner| inner.poll_change(waker)) {
            Some(Poll::Ready(None)) => {
                Pin::set(self.inner(), None);
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
