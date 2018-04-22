use std::marker::PhantomData;
use futures_core::task::Context;
use futures_core::{Async, Poll, Never};
use futures_core::future::{Future, IntoFuture};
use futures_util::stream;
use futures_core::stream::Stream;
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
    #[inline]
    fn to_stream(self) -> SignalStream<Self, Never>
        where Self: Sized {
        SignalStream {
            signal: self,
            phantom: PhantomData,
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

        let poll = match self.inner {
            Some(ref mut inner) => inner.poll_change(cx),

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
