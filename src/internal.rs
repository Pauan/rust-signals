use super::signal::Signal;
use std::pin::Pin;
use std::task::{Poll, Context};
use pin_project::pin_project;


#[inline]
fn unwrap_mut<A>(x: &mut Option<A>) -> &mut A {
    match *x {
        Some(ref mut x) => x,
        None => unreachable!(),
    }
}

#[inline]
fn unwrap_ref<A>(x: &Option<A>) -> &A {
    match *x {
        Some(ref x) => x,
        None => unreachable!(),
    }
}


#[derive(Debug)]
pub struct PollResult {
    pub done: bool,
    pub changed: bool,
}

impl PollResult {
    #[inline]
    pub fn merge(self, other: Self) -> Self {
        Self {
            done: self.done && other.done,
            changed: self.changed || other.changed,
        }
    }
}


#[pin_project(project = MapRef1Proj)]
#[derive(Debug)]
pub struct MapRef1<S> where S: Signal {
    #[pin]
    signal: Option<S>,
    value: Option<S::Item>,
}

impl<S> MapRef1<S> where S: Signal {
    #[inline]
    pub fn new(signal: S) -> Self {
        Self {
            signal: Some(signal),
            value: None,
        }
    }

    /// DO NOT USE!
    #[inline]
    pub fn unsafe_pin(&mut self) -> Pin<&mut Self> {
        // This is safe because it's only used by `map_ref`, and `map_ref` upholds the Pin contract
        // TODO verify that this is 100% safe
        unsafe { ::std::pin::Pin::new_unchecked(self) }
    }

    pub fn poll(self: Pin<&mut Self>, cx: &mut Context) -> PollResult {
        let mut this = self.project();

        match this.signal.as_mut().as_pin_mut().map(|signal| signal.poll_change(cx)) {
            None => {
                PollResult {
                    changed: false,
                    done: true,
                }
            },
            Some(Poll::Ready(None)) => {
                this.signal.set(None);

                PollResult {
                    changed: false,
                    done: true,
                }
            },
            Some(Poll::Ready(a)) => {
                *this.value = a;

                PollResult {
                    changed: true,
                    done: false,
                }
            },
            Some(Poll::Pending) => {
                PollResult {
                    changed: false,
                    done: false,
                }
            },
        }
    }

    #[inline]
    pub fn value_ref(&self) -> &S::Item {
        unwrap_ref(&self.value)
    }

    #[inline]
    pub fn value_mut(self: Pin<&mut Self>) -> &mut S::Item {
        unwrap_mut(self.project().value)
    }
}


#[pin_project(project = MapRefSignalProj)]
#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct MapRefSignal<F> {
    callback: F,
}

impl<A, F> MapRefSignal<F> where F: FnMut(&mut Context) -> Poll<Option<A>> {
    #[inline]
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<A, F> Signal for MapRefSignal<F> where F: FnMut(&mut Context) -> Poll<Option<A>> {
    type Item = A;

    #[inline]
    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.project();
        (this.callback)(cx)
    }
}


// Poll left  => Has left   => Poll right  => Has right   => Output
// -----------------------------------------------------------------------------
// Some(left) =>            => Some(right) =>             => Some((left, right))
// Some(left) =>            => None        => Some(right) => Some((left, right))
// Some(left) =>            => Pending     => Some(right) => Some((left, right))
// None       => Some(left) => Some(right) =>             => Some((left, right))
// Pending    => Some(left) => Some(right) =>             => Some((left, right))
// None       => Some(left) => None        => Some(right) => None
// None       => None       =>             =>             => None
//            =>            => None        => None        => None
// Some(left) =>            => Pending     => None        => Pending
// None       => Some(left) => Pending     => Some(right) => Pending
// None       => Some(left) => Pending     => None        => Pending
// Pending    => Some(left) => None        => Some(right) => Pending
// Pending    => Some(left) => Pending     => Some(right) => Pending
// Pending    => Some(left) => Pending     => None        => Pending
// Pending    => None       => Some(right) =>             => Pending
// Pending    => None       => None        => Some(right) => Pending
// Pending    => None       => Pending     => Some(right) => Pending
// Pending    => None       => Pending     => None        => Pending
