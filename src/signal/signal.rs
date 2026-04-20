use std::pin::Pin;
use std::marker::Unpin;
use std::future::Future;
use std::task::{Context, Poll};
use futures_core::stream::Stream;
use futures_util::stream;
use futures_util::stream::StreamExt;
use pin_project::pin_project;

use crate::signal::Broadcaster;
use crate::signal_vec::{VecDiff, SignalVec};


// TODO impl for AssertUnwindSafe ?
// TODO documentation for Signal contract:
// * a Signal must always return Poll::Ready(Some(...)) the first time it is polled, no exceptions
// * after the first time it can then return Poll::Ready(None) which means that the Signal is ended (i.e. there won't be any future changes)
// * or it can return Poll::Pending, which means the Signal hasn't changed from its previous value
// * whenever the Signal's value has changed, it must call cx.waker().wake_by_ref() which will notify the consumer that the Signal has changed
// * If wake_by_ref() hasn't been called, then the consumer assumes that nothing has changed, so it won't re-poll the Signal
// * unlike Streams, the consumer does not poll again if it receives Poll::Ready(Some(...)), it will only repoll if wake_by_ref() is called
// * If the Signal returns Poll::Ready(None) then the consumer must not re-poll the Signal
#[must_use = "Signals do nothing unless polled"]
pub trait Signal {
    type Item;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>>;
}


// Copied from Future in the Rust stdlib
impl<'a, A> Signal for &'a mut A where A: ?Sized + Signal + Unpin {
    type Item = A::Item;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        A::poll_change(Pin::new(&mut **self), cx)
    }
}

// Copied from Future in the Rust stdlib
impl<A> Signal for Box<A> where A: ?Sized + Signal + Unpin {
    type Item = A::Item;

    #[inline]
    fn poll_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        A::poll_change(Pin::new(&mut *self), cx)
    }
}

// Copied from Future in the Rust stdlib
impl<A> Signal for Pin<A>
    where A: Unpin + ::std::ops::DerefMut,
          A::Target: Signal {
    type Item = <<A as ::std::ops::Deref>::Target as Signal>::Item;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        Pin::get_mut(self).as_mut().poll_change(cx)
    }
}


// TODO Seal this
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

    #[inline]
    fn eq(self, value: Self::Item) -> Eq<Self>
        where Self::Item: PartialEq,
              Self: Sized {
        Eq {
            signal: self,
            matches: None,
            value,
        }
    }

    #[inline]
    fn neq(self, value: Self::Item) -> Neq<Self>
        where Self::Item: PartialEq,
              Self: Sized {
        Neq {
            signal: self,
            matches: None,
            value,
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
              Self::Item: PartialEq,
              Self: Sized {
        DedupeMap {
            old_value: None,
            signal: self,
            callback,
        }
    }

    #[inline]
    fn dedupe(self) -> Dedupe<Self>
        where Self::Item: PartialEq,
              Self: Sized {
        Dedupe {
            old_value: None,
            signal: self,
        }
    }

    #[inline]
    fn dedupe_cloned(self) -> DedupeCloned<Self>
        where Self::Item: PartialEq,
              Self: Sized {
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

    /// Creates a `Signal` which delays updates until a `Future` finishes.
    ///
    /// This can be used to throttle a `Signal` so that it only updates once every X seconds.
    ///
    /// If multiple updates happen while it's being delayed, it will only output the most recent
    /// value.
    ///
    /// # Examples
    ///
    /// Wait 1 second between each update:
    ///
    /// ```rust
    /// # use core::future::Future;
    /// # use futures_signals::signal::{always, SignalExt};
    /// # fn sleep(ms: i32) -> impl Future<Output = ()> { async {} }
    /// # let input = always(1);
    /// let output = input.throttle(|| sleep(1_000));
    /// ```
    ///
    /// # Performance
    ///
    /// This is ***extremely*** efficient: it does not do any heap allocation, and it has *very* little overhead.
    #[inline]
    fn throttle<A, B>(self, callback: B) -> Throttle<Self, A, B>
        where A: Future<Output = ()>,
              B: FnMut() -> A,
              Self: Sized {
        Throttle {
            signal: Some(self),
            future: None,
            callback,
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
    fn switch_signal_vec<A, F>(self, callback: F) -> SwitchSignalVec<Self, A, F>
        where A: SignalVec,
              F: FnMut(Self::Item) -> A,
              Self: Sized {
        SwitchSignalVec {
            signal: Some(self),
            signal_vec: None,
            callback,
            len: 0,
        }
    }

    /// Creates a `Stream` which samples the value of `self` whenever the `Stream` has a new value.
    ///
    /// # Performance
    ///
    /// This is ***extremely*** efficient: it does not do any heap allocation, and it has *very* little overhead.
    #[inline]
    fn sample_stream_cloned<A>(self, stream: A) -> SampleStreamCloned<Self, A>
        where A: Stream,
              A::Item: Clone,
              Self: Sized {
        SampleStreamCloned {
            signal: Some(self),
            stream: stream,
            value: None,
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

    /// Conditionally stops the `Signal`.
    ///
    /// For each value in `self` it will call the `test` function.
    ///
    /// If `test` returns `true` then the `Signal` will stop emitting
    /// any future values.
    ///
    /// The value which is passed to `test` is always emitted no matter
    /// what.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use futures_signals::signal::{always, SignalExt};
    /// # let input = always(1);
    /// // Stops the signal when x is above 5
    /// let output = input.stop_if(|x| *x > 5);
    /// ```
    ///
    /// # Performance
    ///
    /// This is ***extremely*** efficient: it is *guaranteed* constant time, and it does not do
    /// any heap allocation.
    #[inline]
    fn stop_if<F>(self, test: F) -> StopIf<Self, F>
        where F: FnMut(&Self::Item) -> bool,
              Self: Sized {
        StopIf {
            signal: self,
            stopped: false,
            test,
        }
    }


    #[inline]
    #[track_caller]
    #[cfg(feature = "debug")]
    fn debug(self) -> SignalDebug<Self> where Self: Sized, Self::Item: std::fmt::Debug {
        SignalDebug {
            signal: self,
            location: std::panic::Location::caller(),
        }
    }

    /// A convenience method for calling [`Broadcaster::new`].
    ///
    /// This allows you to clone / split a `Signal` into multiple `Signal`s.
    ///
    /// See the documentation for [`Broadcaster`] for more details.
    #[inline]
    fn broadcast(self) -> Broadcaster<Self> where Self: Sized {
        Broadcaster::new(self)
    }

    /// A convenience for calling `Signal::poll_change` on `Unpin` types.
    #[inline]
    fn poll_change_unpin(&mut self, cx: &mut Context) -> Poll<Option<Self::Item>> where Self: Unpin + Sized {
        Pin::new(self).poll_change(cx)
    }

    #[inline]
    fn boxed<'a>(self) -> Pin<Box<dyn Signal<Item = Self::Item> + Send + 'a>>
        where Self: Sized + Send + 'a {
        Box::pin(self)
    }

    #[inline]
    fn boxed_sync<'a>(self) -> Pin<Box<dyn Signal<Item = Self::Item> + Send + Sync + 'a>>
        where Self: Sized + Send + Sync + 'a {
        Box::pin(self)
    }

    #[inline]
    fn boxed_local<'a>(self) -> Pin<Box<dyn Signal<Item = Self::Item> + 'a>>
        where Self: Sized + 'a {
        Box::pin(self)
    }
}

// TODO why is this ?Sized
impl<T: ?Sized> SignalExt for T where T: Signal {}


/// An owned dynamically typed [`Signal`].
///
/// This is useful if you don't know the static type, or if you need
/// indirection.
pub type BoxSignal<'a, T> = Pin<Box<dyn Signal<Item = T> + Send + 'a>>;

/// Same as [`BoxSignal`], but with a `Sync` requirement.
pub type SyncBoxSignal<'a, T> = Pin<Box<dyn Signal<Item = T> + Send + Sync + 'a>>;

/// Same as [`BoxSignal`], but without the `Send` requirement.
pub type LocalBoxSignal<'a, T> = Pin<Box<dyn Signal<Item = T> + 'a>>;


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
    crate::map_ref! {
        let a = left,
        let b = right =>
        *a && *b
    }
}

// TODO make this into a method later
// TODO use short-circuiting if the left signal returns true ?
#[inline]
pub fn or<A, B>(left: A, right: B) -> impl Signal<Item = bool>
    where A: Signal<Item = bool>,
          B: Signal<Item = bool> {
    crate::map_ref! {
        let a = left,
        let b = right =>
        *a || *b
    }
}


#[pin_project(project = MaybeSignalStateProj)]
#[derive(Debug)]
enum MaybeSignalState<S, E> {
    Signal(#[pin] S),
    Value(Option<E>),
}

impl<S, E> MaybeSignalState<S, E> where S: Signal {
    fn poll<A, B, C>(self: Pin<&mut Self>, cx: &mut Context, map_signal: A, map_value: B) -> Poll<Option<C>>
        where A: FnOnce(S::Item) -> C,
              B: FnOnce(E) -> C {
        match self.project() {
            MaybeSignalStateProj::Signal(signal) => {
                signal.poll_change(cx).map(|value| value.map(map_signal))
            },
            MaybeSignalStateProj::Value(value) => {
                match value.take() {
                    Some(value) => Poll::Ready(Some(map_value(value))),
                    None => Poll::Ready(None),
                }
            },
        }
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct ResultSignal<S, E> {
    #[pin]
    state: MaybeSignalState<S, E>,
}

impl<S, E> Signal for ResultSignal<S, E> where S: Signal {
    type Item = Result<S::Item, E>;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().state.poll(cx, Ok, Err)
    }
}

/// Converts a `Result<Signal<A>, B>` into a `Signal<Result<A, B>>`.
///
/// This is mostly useful with [`SignalExt::switch`] or [`SignalExt::flatten`].
///
/// If the value is `Err(value)` then it behaves like [`always`], it just returns that
/// value.
///
/// If the value is `Ok(signal)` then it will return the result of the signal,
/// except wrapped in `Ok`.
pub fn result<S, E>(value: Result<S, E>) -> ResultSignal<S, E> where S: Signal {
    match value {
        Ok(signal) => ResultSignal {
            state: MaybeSignalState::Signal(signal),
        },
        Err(value) => ResultSignal {
            state: MaybeSignalState::Value(Some(value)),
        },
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct OptionSignal<S> where S: Signal {
    #[pin]
    state: MaybeSignalState<S, Option<S::Item>>,
}

impl<S> Signal for OptionSignal<S> where S: Signal {
    type Item = Option<S::Item>;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().state.poll(cx, Some, |x| x)
    }
}

/// Converts an `Option<Signal<A>>` into a `Signal<Option<A>>`.
///
/// This is mostly useful with [`SignalExt::switch`] or [`SignalExt::flatten`].
///
/// If the value is `None` then it behaves like [`always`], it just returns `None`.
///
/// If the value is `Some(signal)` then it will return the result of the signal,
/// except wrapped in `Some`.
pub fn option<S>(value: Option<S>) -> OptionSignal<S> where S: Signal {
    match value {
        Some(signal) => OptionSignal {
            state: MaybeSignalState::Signal(signal),
        },
        None => OptionSignal {
            state: MaybeSignalState::Value(Some(None)),
        },
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
#[cfg(feature = "debug")]
pub struct SignalDebug<A> {
    #[pin]
    signal: A,
    location: &'static std::panic::Location<'static>,
}

#[cfg(feature = "debug")]
impl<A> Signal for SignalDebug<A> where A: Signal, A::Item: std::fmt::Debug {
    type Item = A::Item;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.project();

        let poll = this.signal.poll_change(cx);

        log::trace!("[{}] {:#?}", this.location, poll);

        poll
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct FromFuture<A> {
    // TODO is this valid with pinned types ?
    #[pin]
    future: Option<A>,
    first: bool,
}

impl<A> Signal for FromFuture<A> where A: Future {
    type Item = Option<A::Output>;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        // TODO is this valid with pinned types ?
        match this.future.as_mut().as_pin_mut().map(|future| future.poll(cx)) {
            None => {
                Poll::Ready(None)
            },

            Some(Poll::Ready(value)) => {
                this.future.set(None);
                Poll::Ready(Some(Some(value)))
            },

            Some(Poll::Pending) => {
                if *this.first {
                    *this.first = false;
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


#[pin_project]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct FromStream<A> {
    #[pin]
    stream: Option<A>,
    first: bool,
}

impl<A> Signal for FromStream<A> where A: Stream {
    type Item = Option<A::Item>;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        let mut value = None;

        let done = loop {
            match this.stream.as_mut().as_pin_mut().map(|stream| stream.poll_next(cx)) {
                None => {
                    break true;
                },

                Some(Poll::Ready(None)) => {
                    this.stream.set(None);
                    break true;
                },

                Some(Poll::Ready(Some(new_value))) => {
                    value = Some(new_value);
                    continue;
                },

                Some(Poll::Pending) => {
                    break false;
                },
            }
        };

        match value {
            Some(value) => {
                *this.first = false;
                Poll::Ready(Some(Some(value)))
            },
            None => {
                if *this.first {
                    *this.first = false;
                    Poll::Ready(Some(None))

                } else if done {
                    Poll::Ready(None)

                } else {
                    Poll::Pending
                }
            },
        }
    }
}

#[inline]
pub fn from_stream<A>(stream: A) -> FromStream<A> where A: Stream {
    FromStream { stream: Some(stream), first: true }
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
    fn poll_change(mut self: Pin<&mut Self>, _: &mut Context) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.value.take())
    }
}

#[inline]
pub fn always<A>(value: A) -> Always<A> {
    Always {
        value: Some(value),
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct First<A> {
    #[pin]
    signal: Option<A>,
}

impl<A> Signal for First<A> where A: Signal {
    type Item = A::Item;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        // TODO maybe it's safe to replace this with take ?
        if let Some(poll) = this.signal.as_mut().as_pin_mut().map(|signal| signal.poll_change(cx)) {
            this.signal.set(None);
            poll

        } else {
            Poll::Ready(None)
        }
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Switch<A, B, C> where A: Signal, C: FnMut(A::Item) -> B {
    #[pin]
    inner: Flatten<Map<A, C>>,
}

impl<A, B, C> Signal for Switch<A, B, C>
    where A: Signal,
          B: Signal,
          C: FnMut(A::Item) -> B {
    type Item = B::Item;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_change(cx)
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Streams do nothing unless polled"]
pub struct SampleStreamCloned<A, B> where A: Signal, B: Stream {
    #[pin]
    signal: Option<A>,
    #[pin]
    stream: B,
    value: Option<A::Item>,
}

impl<A, B> Stream for SampleStreamCloned<A, B>
    where A: Signal,
          A::Item: Clone,
          B: Stream {
    type Item = (A::Item, B::Item);

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        this.stream.as_mut().poll_next(cx).map(|option| {
            option.map(|value| {
                loop {
                    break match this.signal.as_mut().as_pin_mut().map(|signal| signal.poll_change(cx)) {
                        Some(Poll::Ready(Some(value))) => {
                            *this.value = Some(value);
                            continue;
                        },
                        Some(Poll::Ready(None)) => {
                            this.signal.set(None);
                        },
                        _ => {},
                    };
                }

                match this.value {
                    Some(ref signal) => {
                        (signal.clone(), value)
                    },
                    None => {
                        unreachable!()
                    },
                }
            })
        })
    }
}


// TODO faster for_each which doesn't poll twice on Poll::Ready
#[pin_project]
#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct ForEach<A, B, C> {
    #[pin]
    inner: stream::ForEach<SignalStream<A>, B, C>,
}

impl<A, B, C> Future for ForEach<A, B, C>
    where A: Signal,
          B: Future<Output = ()>,
          C: FnMut(A::Item) -> B {
    type Output = ();

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Streams do nothing unless polled"]
pub struct SignalStream<A> {
    #[pin]
    signal: A,
}

impl<A: Signal> Stream for SignalStream<A> {
    type Item = A::Item;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().signal.poll_change(cx)
    }
}


// TODO maybe remove this ?
#[pin_project]
#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct SignalFuture<A> where A: Signal {
    #[pin]
    signal: A,
    value: Option<A::Item>,
}

impl<A> Future for SignalFuture<A> where A: Signal {
    type Output = A::Item;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            return match this.signal.as_mut().poll_change(cx) {
                Poll::Ready(None) => {
                    Poll::Ready(this.value.take().unwrap())
                },
                Poll::Ready(Some(new_value)) => {
                    *this.value = Some(new_value);
                    continue;
                },
                Poll::Pending => {
                    Poll::Pending
                },
            }
        }
    }
}


#[pin_project(project = MapProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Map<A, B> {
    #[pin]
    signal: A,
    callback: B,
}

impl<A, B, C> Signal for Map<A, B>
    where A: Signal,
          B: FnMut(A::Item) -> C {
    type Item = C;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let MapProj { signal, callback } = self.project();

        signal.poll_change(cx).map(|opt| opt.map(|value| callback(value)))
    }
}


#[pin_project(project = StopIfProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct StopIf<A, B> {
    #[pin]
    signal: A,
    stopped: bool,
    test: B,
}

impl<A, B> Signal for StopIf<A, B>
    where A: Signal,
          B: FnMut(&A::Item) -> bool {
    type Item = A::Item;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let StopIfProj { signal, stopped, test } = self.project();

        if *stopped {
            Poll::Ready(None)

        } else {
            match signal.poll_change(cx) {
                Poll::Ready(Some(value)) => {
                    if test(&value) {
                        *stopped = true;
                    }

                    Poll::Ready(Some(value))
                },
                Poll::Ready(None) => {
                    *stopped = true;
                    Poll::Ready(None)
                },
                Poll::Pending => Poll::Pending,
            }
        }
    }
}


#[pin_project(project = EqProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Eq<A> where A: Signal {
    #[pin]
    signal: A,
    matches: Option<bool>,
    value: A::Item,
}

impl<A> Signal for Eq<A>
    where A: Signal,
          A::Item: PartialEq {
    type Item = bool;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let EqProj { mut signal, matches, value } = self.project();

        loop {
            return match signal.as_mut().poll_change(cx) {
                Poll::Ready(Some(new_value)) => {
                    let new = Some(new_value == *value);

                    if *matches != new {
                        *matches = new;
                        Poll::Ready(new)

                    } else {
                        continue;
                    }
                },
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}


#[pin_project(project = NeqProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Neq<A> where A: Signal {
    #[pin]
    signal: A,
    matches: Option<bool>,
    value: A::Item,
}

impl<A> Signal for Neq<A>
    where A: Signal,
          A::Item: PartialEq {
    type Item = bool;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let NeqProj { mut signal, matches, value } = self.project();

        loop {
            return match signal.as_mut().poll_change(cx) {
                Poll::Ready(Some(new_value)) => {
                    let new = Some(new_value != *value);

                    if *matches != new {
                        *matches = new;
                        Poll::Ready(new)

                    } else {
                        continue;
                    }
                },
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Inspect<A, B> {
    #[pin]
    signal: A,
    callback: B,
}

impl<A, B> Signal for Inspect<A, B>
    where A: Signal,
          B: FnMut(&A::Item) {
    type Item = A::Item;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.project();

        let poll = this.signal.poll_change(cx);

        if let Poll::Ready(Some(ref value)) = poll {
            (this.callback)(value);
        }

        poll
    }
}


#[pin_project(project = MapFutureProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct MapFuture<A, B, C> {
    #[pin]
    signal: Option<A>,
    #[pin]
    future: Option<B>,
    callback: C,
    first: bool,
}

impl<A, B, C> Signal for MapFuture<A, B, C>
    where A: Signal,
          B: Future,
          C: FnMut(A::Item) -> B {
    type Item = Option<B::Output>;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let MapFutureProj { mut signal, mut future, callback, first } = self.project();

        let mut done = false;

        loop {
            match signal.as_mut().as_pin_mut().map(|signal| signal.poll_change(cx)) {
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

        match future.as_mut().as_pin_mut().map(|future| future.poll(cx)) {
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


#[pin_project(project = ThrottleProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Throttle<A, B, C> where A: Signal {
    #[pin]
    signal: Option<A>,
    #[pin]
    future: Option<B>,
    callback: C,
}

impl<A, B, C> Signal for Throttle<A, B, C>
    where A: Signal,
          B: Future<Output = ()>,
          C: FnMut() -> B {
    type Item = A::Item;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let ThrottleProj { mut signal, mut future, callback } = self.project();

        match future.as_mut().as_pin_mut().map(|future| future.poll(cx)) {
            None => {},
            Some(Poll::Ready(())) => {
                future.set(None);
            },
            Some(Poll::Pending) => {
                // TODO does this need to poll the Signal as well ?
                return Poll::Pending;
            },
        }

        match signal.as_mut().as_pin_mut().map(|signal| signal.poll_change(cx)) {
            None => {
                Poll::Ready(None)
            },
            Some(Poll::Ready(None)) => {
                // TODO maybe remove the future too ?
                signal.set(None);
                Poll::Ready(None)
            },
            Some(Poll::Ready(Some(value))) => {
                future.set(Some(callback()));

                if let Some(Poll::Ready(())) = future.as_mut().as_pin_mut().map(|future| future.poll(cx)) {
                    future.set(None);
                }

                Poll::Ready(Some(value))
            },
            Some(Poll::Pending) => {
                Poll::Pending
            },
        }
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct WaitFor<A>
    where A: Signal,
          A::Item: PartialEq {
    #[pin]
    signal: A,
    value: A::Item,
}

impl<A> Future for WaitFor<A>
    where A: Signal,
          A::Item: PartialEq {

    type Output = Option<A::Item>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            let poll = this.signal.as_mut().poll_change(cx);

            if let Poll::Ready(Some(ref new_value)) = poll {
                if new_value != this.value {
                    continue;
                }
            }

            return poll;
        }
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct SignalSignalVec<A> {
    #[pin]
    signal: A,
}

impl<A, B> SignalVec for SignalSignalVec<A>
    where A: Signal<Item = Vec<B>> {
    type Item = B;

    #[inline]
    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        self.project().signal.poll_change(cx).map(|opt| opt.map(|values| VecDiff::Replace { values }))
    }
}


// TODO should this inline ?
fn dedupe<A, S, F>(mut signal: Pin<&mut S>, cx: &mut Context, old_value: &mut Option<S::Item>, f: F) -> Poll<Option<A>>
    where S: Signal,
          S::Item: PartialEq,
          F: FnOnce(&mut S::Item) -> A {
    loop {
        return match signal.as_mut().poll_change(cx) {
            Poll::Ready(Some(mut new_value)) => {
                let has_changed = match old_value {
                    Some(old_value) => *old_value != new_value,
                    None => true,
                };

                if has_changed {
                    let output = f(&mut new_value);
                    *old_value = Some(new_value);
                    Poll::Ready(Some(output))

                } else {
                    continue;
                }
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}


#[pin_project(project = DedupeMapProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct DedupeMap<A, B> where A: Signal {
    old_value: Option<A::Item>,
    #[pin]
    signal: A,
    callback: B,
}

impl<A, B, C> Signal for DedupeMap<A, B>
    where A: Signal,
          A::Item: PartialEq,
          // TODO should this use & instead of &mut ?
          // TODO should this use Fn instead ?
          B: FnMut(&mut A::Item) -> C {

    type Item = C;

    // TODO should this use #[inline] ?
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let DedupeMapProj { old_value, signal, callback } = self.project();

        dedupe(signal, cx, old_value, callback)
    }
}


#[pin_project(project = DedupeProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Dedupe<A> where A: Signal {
    old_value: Option<A::Item>,
    #[pin]
    signal: A,
}

impl<A> Signal for Dedupe<A>
    where A: Signal,
          A::Item: PartialEq + Copy {

    type Item = A::Item;

    // TODO should this use #[inline] ?
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let DedupeProj { old_value, signal } = self.project();

        dedupe(signal, cx, old_value, |value| *value)
    }
}


#[pin_project(project = DedupeClonedProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct DedupeCloned<A> where A: Signal {
    old_value: Option<A::Item>,
    #[pin]
    signal: A,
}

impl<A> Signal for DedupeCloned<A>
    where A: Signal,
          A::Item: PartialEq + Clone {

    type Item = A::Item;

    // TODO should this use #[inline] ?
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let DedupeClonedProj { old_value, signal } = self.project();

        dedupe(signal, cx, old_value, |value| value.clone())
    }
}


#[pin_project(project = FilterMapProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct FilterMap<A, B> {
    #[pin]
    signal: A,
    callback: B,
    first: bool,
}

impl<A, B, C> Signal for FilterMap<A, B>
    where A: Signal,
          B: FnMut(A::Item) -> Option<C> {
    type Item = Option<C>;

    // TODO should this use #[inline] ?
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let FilterMapProj { mut signal, callback, first } = self.project();

        loop {
            return match signal.as_mut().poll_change(cx) {
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


// TODO test the Unpin impl of this
//      impl<A> Unpin for Flatten<A> where A: Unpin + Signal, A::Item: Unpin {}
#[pin_project(project = FlattenProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Flatten<A> where A: Signal {
    #[pin]
    signal: Option<A>,
    #[pin]
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
    type Item = <A::Item as Signal>::Item;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let FlattenProj { mut signal, mut inner } = self.project();

        let done = match signal.as_mut().as_pin_mut().map(|signal| signal.poll_change(cx)) {
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

        match inner.as_mut().as_pin_mut().map(|inner| inner.poll_change(cx)) {
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


#[pin_project(project = SwitchSignalVecProj)]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct SwitchSignalVec<A, B, C> where B: SignalVec {
    #[pin]
    signal: Option<A>,
    #[pin]
    signal_vec: Option<B>,
    callback: C,
    len: usize,
}

impl<A, B, C> SignalVec for SwitchSignalVec<A, B, C>
    where A: Signal,
          B: SignalVec,
          C: FnMut(A::Item) -> B {
    type Item = B::Item;

    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
        let SwitchSignalVecProj { mut signal, mut signal_vec, callback, len } = self.project();

        let mut signal_value = None;

        let signal_done = loop {
            break match signal.as_mut().as_pin_mut().map(|signal| signal.poll_change(cx)) {
                None => {
                    true
                },
                Some(Poll::Pending) => {
                    false
                },
                Some(Poll::Ready(None)) => {
                    signal.set(None);
                    true
                },
                Some(Poll::Ready(Some(value))) => {
                    signal_value = Some(value);
                    continue;
                },
            }
        };

        fn new_signal_vec<A>(len: &mut usize) -> Poll<Option<VecDiff<A>>> {
            if *len == 0 {
                Poll::Pending

            } else {
                *len = 0;
                Poll::Ready(Some(VecDiff::Replace { values: vec![] }))
            }
        }

        fn calculate_len<A>(len: &mut usize, vec_diff: &VecDiff<A>) {
            match vec_diff {
                VecDiff::Replace { values } => {
                    *len = values.len();
                },
                VecDiff::InsertAt { .. } | VecDiff::Push { .. } => {
                    *len += 1;
                },
                VecDiff::RemoveAt { .. } | VecDiff::Pop {} => {
                    *len -= 1;
                },
                VecDiff::Clear {} => {
                    *len = 0;
                },
                VecDiff::UpdateAt { .. } | VecDiff::Move { .. } => {},
            }
        }

        if let Some(value) = signal_value {
            signal_vec.set(Some(callback(value)));

            match signal_vec.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(cx)) {
                None => {
                    if signal_done {
                        Poll::Ready(None)

                    } else {
                        new_signal_vec(len)
                    }
                },

                Some(Poll::Pending) => {
                    new_signal_vec(len)
                },

                Some(Poll::Ready(None)) => {
                    signal_vec.set(None);

                    if signal_done {
                        Poll::Ready(None)

                    } else {
                        new_signal_vec(len)
                    }
                },

                Some(Poll::Ready(Some(vec_diff))) => {
                    if *len == 0 {
                        calculate_len(len, &vec_diff);
                        Poll::Ready(Some(vec_diff))

                    } else {
                        let mut values = vec![];

                        vec_diff.apply_to_vec(&mut values);

                        *len = values.len();

                        Poll::Ready(Some(VecDiff::Replace { values }))
                    }
                },
            }

        } else {
            match signal_vec.as_mut().as_pin_mut().map(|signal| signal.poll_vec_change(cx)) {
                None => {
                    if signal_done {
                        Poll::Ready(None)

                    } else {
                        Poll::Pending
                    }
                },

                Some(Poll::Pending) => {
                    Poll::Pending
                },

                Some(Poll::Ready(None)) => {
                    signal_vec.set(None);

                    if signal_done {
                        Poll::Ready(None)

                    } else {
                        Poll::Pending
                    }
                },

                Some(Poll::Ready(Some(vec_diff))) => {
                    calculate_len(len, &vec_diff);
                    Poll::Ready(Some(vec_diff))
                },
            }
        }
    }
}
