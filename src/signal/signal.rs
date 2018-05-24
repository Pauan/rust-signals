use std::marker::PhantomData;
use futures_core::task::Context;
use futures_core::{Async, Poll, Never};
use futures_core::future::{Future, IntoFuture};
use futures_core::stream::Stream;
use futures_util::stream;
use futures_util::stream::StreamExt;
use signal_vec::{VecDiff, SignalVec};


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

    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>>;
}

impl<F: ?Sized + Signal> Signal for ::std::boxed::Box<F> {
    type Item = F::Item;

    #[inline]
    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        (**self).poll_change(cx)
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
    fn to_stream(self) -> SignalStream<Self, Never>
        where Self: Sized {
        SignalStream {
            signal: self,
            phantom: PhantomData,
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
    /// Because `map_dedupe` has the same behavior as `map`, it is useful solely as a performance optimization.
    ///
    /// # Performance
    ///
    /// The performance is the same as `map`, except with an additional call to `eq`.
    ///
    /// If the `eq` call is fast, then `map_dedupe` can be faster than `map`, because it doesn't call the closure
    /// when the new and old values are the same, and it also doesn't update any child Signals.
    ///
    /// On the other hand, if the `eq` call is slow, then `map_dedupe` is probably slower than `map`.
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

    /// Creates a `Signal` which uses a closure to asynchronously transform the value.
    ///
    /// When the output `Signal` is spawned:
    ///
    /// 1. It calls the closure with the current value of `self`.
    ///
    /// 2. The closure returns an `IntoFuture`. It waits for that `Future` to finish, and then
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
    /// # extern crate futures_signals;
    /// # use futures_core::Never;
    /// # use futures_signals::signal::{always, SignalExt};
    /// # fn call_network_api(value: u32) -> Result<(), Never> { Ok(()) }
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
        where A: IntoFuture<Error = Never>,
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
    fn for_each<U, F>(self, callback: F) -> ForEach<Self, U, F>
        where U: IntoFuture<Item = ()>,
              F: FnMut(Self::Item) -> U,
              Self: Sized {
        // TODO a bit hacky
        ForEach {
            inner: SignalStream {
                signal: self,
                phantom: PhantomData,
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
}

// TODO why is this ?Sized
impl<T: ?Sized> SignalExt for T where T: Signal {}


pub struct FromFuture<A> {
    future: Option<A>,
    first: bool,
}

impl<A> Signal for FromFuture<A> where A: Future<Error = Never> {
    type Item = Option<A::Item>;

    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        match self.future.as_mut().map(|future| future.poll(cx)) {
            None => {
                Async::Ready(None)
            },

            Some(Ok(Async::Ready(value))) => {
                self.future = None;
                Async::Ready(Some(Some(value)))
            },

            Some(Ok(Async::Pending)) => {
                if self.first {
                    self.first = false;
                    Async::Ready(Some(None))

                } else {
                    Async::Pending
                }
            },

            Some(Err(_)) => unreachable!(),
        }
    }
}

#[inline]
pub fn from_future<A>(future: A) -> FromFuture<A::Future> where A: IntoFuture {
    FromFuture { future: Some(future.into_future()), first: true }
}


pub struct FromStream<A> {
    stream: A,
    first: bool,
}

impl<A> Signal for FromStream<A> where A: Stream<Error = Never> {
    type Item = Option<A::Item>;

    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        match self.stream.poll_next(cx) {
            Ok(Async::Ready(None)) => {
                Async::Ready(None)
            },

            Ok(Async::Ready(Some(value))) => {
                Async::Ready(Some(Some(value)))
            },

            Ok(Async::Pending) => {
                if self.first {
                    self.first = false;
                    Async::Ready(Some(None))

                } else {
                    Async::Pending
                }
            },

            Err(_) => unreachable!(),
        }
    }
}

#[inline]
pub fn from_stream<A>(stream: A) -> FromStream<A> where A: Stream {
    FromStream { stream, first: true }
}


pub struct Always<A> {
    value: Option<A>,
}

impl<A> Signal for Always<A> {
    type Item = A;

    #[inline]
    fn poll_change(&mut self, _: &mut Context) -> Async<Option<Self::Item>> {
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
    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        self.inner.poll_change(cx)
    }
}


pub struct ForEach<A, B, C> where B: IntoFuture {
    inner: stream::ForEach<SignalStream<A, B::Error>, B, C>,
}

impl<A, B, C> Future for ForEach<A, B, C>
    where A: Signal,
          B: IntoFuture<Item = ()>,
          C: FnMut(A::Item) -> B {
    type Item = ();
    type Error = B::Error;

    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Poll<Self::Item, Self::Error> {
        // TODO a teensy bit hacky
        self.inner.poll(cx).map(|async| async.map(|_| ()))
    }
}


pub struct SignalStream<A, Error> {
    signal: A,
    phantom: PhantomData<Error>,
}

impl<A: Signal, Error> Stream for SignalStream<A, Error> {
    type Item = A::Item;
    type Error = Error;

    #[inline]
    fn poll_next(&mut self, cx: &mut Context) -> Poll<Option<Self::Item>, Self::Error> {
        Ok(self.signal.poll_change(cx))
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
    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        self.signal.poll_change(cx).map(|opt| opt.map(|value| (self.callback)(value)))
    }
}


pub struct MapFuture<A, B, C> where B: IntoFuture<Error = Never> {
    signal: Option<A>,
    future: Option<B::Future>,
    callback: C,
    first: bool,
}

impl<A, B, C> Signal for MapFuture<A, B, C>
    where A: Signal,
          B: IntoFuture<Error = Never>,
          C: FnMut(A::Item) -> B {
    type Item = Option<B::Item>;

    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        let mut done = false;

        loop {
            match self.signal.as_mut().map(|signal| signal.poll_change(cx)) {
                None => {
                    done = true;
                    break;
                },
                Some(Async::Ready(None)) => {
                    self.signal = None;
                    done = true;
                    break;
                },
                Some(Async::Ready(Some(value))) => {
                    self.future = Some((self.callback)(value).into_future());
                },
                Some(Async::Pending) => {
                    break;
                },
            }
        }

        match self.future.as_mut().map(|future| future.poll(cx)) {
            None => {},
            Some(Ok(Async::Ready(value))) => {
                self.future = None;
                return Async::Ready(Some(Some(value)));
            },
            Some(Ok(Async::Pending)) => {
                done = false;
            },
            Some(Err(_)) => unreachable!(),
        }

        if self.first {
            self.first = false;
            Async::Ready(Some(None))

        } else if done {
            Async::Ready(None)

        } else {
            Async::Pending
        }
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

    type Item = Option<A::Item>;
    type Error = Never;

    fn poll(&mut self, cx: &mut Context) -> Poll<Self::Item, Self::Error> {
        loop {
            let poll = self.signal.poll_change(cx);

            if let Async::Ready(Some(ref value)) = poll {
                if *value != self.value {
                    continue;
                }
            }

            return Ok(poll);
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
    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecDiff<B>>> {
        self.signal.poll_change(cx).map(|opt| opt.map(|values| VecDiff::Replace { values }))
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
    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        loop {
            return match self.signal.poll_change(cx) {
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
    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        loop {
            return match self.signal.poll_change(cx) {
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
    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        let done = match self.signal.as_mut().map(|signal| signal.poll_change(cx)) {
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

        match self.inner.as_mut().map(|inner| inner.poll_change(cx)) {
            Some(Async::Ready(None)) => {
                self.inner = None;
            },
            Some(poll) => {
                return poll;
            },
            None => {},
        }

        if done {
            Async::Ready(None)

        } else {
            Async::Pending
        }
    }
}
