use super::signal::Signal;
use std::pin::{Unpin, Pin};
// TODO use parking_lot ?
use std::sync::{Arc, RwLock, Mutex, MutexGuard, RwLockReadGuard};
use futures_core::Poll;
use futures_core::task::LocalWaker;


#[inline]
pub fn lock_mut<A>(x: &Mutex<A>) -> MutexGuard<A> {
    x.lock().unwrap()
}

#[inline]
pub fn lock_ref<A>(x: &RwLock<A>) -> RwLockReadGuard<A> {
    x.read().unwrap()
}

pub fn unwrap_mut<A>(x: &mut Option<A>) -> &mut A {
    match *x {
        Some(ref mut x) => x,
        None => unreachable!(),
    }
}

pub fn unwrap_ref<A>(x: &Option<A>) -> &A {
    match *x {
        Some(ref x) => x,
        None => unreachable!(),
    }
}


// TODO figure out another way of doing this
macro_rules! unsafe_pin {
    ($self:expr) => {
        unsafe { ::std::pin::Pin::new_unchecked(&mut $self) }
    }
}

macro_rules! unsafe_unpin {
    ($self:expr) => {
        unsafe { ::std::pin::Pin::get_mut_unchecked(::std::pin::Pin::as_mut(&mut $self)) }
    }
}


#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct Map2<A, B, C> where A: Signal, B: Signal {
    signal1: Option<A>,
    signal2: Option<B>,
    callback: C,
    left: Option<A::Item>,
    right: Option<B::Item>,
}

impl<A, B, C, D> Map2<A, B, C>
    where A: Signal,
          B: Signal,
          C: FnMut(&mut A::Item, &mut B::Item) -> D {
    #[inline]
    pub fn new(left: A, right: B, callback: C) -> Self {
        Self {
            signal1: Some(left),
            signal2: Some(right),
            callback,
            left: None,
            right: None,
        }
    }
}

impl<A, B, C> Unpin for Map2<A, B, C> where A: Unpin + Signal, B: Unpin + Signal {}

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
impl<A, B, C, D> Signal for Map2<A, B, C>
    where A: Signal,
          B: Signal,
          C: FnMut(&mut A::Item, &mut B::Item) -> D {
    type Item = D;

    // TODO inline this ?
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        let mut changed = false;

        // TODO is this safe ?
        let this: &mut Self = unsafe_unpin!(self);

        let left_done = match unsafe_pin!(this.signal1).as_pin_mut().map(|signal| signal.poll_change(waker)) {
            None => true,
            Some(Poll::Ready(None)) => {
                Pin::set(unsafe_pin!(this.signal1), None);
                true
            },
            Some(Poll::Ready(a)) => {
                this.left = a;
                changed = true;
                false
            },
            Some(Poll::Pending) => false,
        };

        let left = match this.left {
            Some(ref mut left) => left,

            None => if left_done {
                return Poll::Ready(None);

            } else {
                return Poll::Pending;
            },
        };

        let right_done = match unsafe_pin!(this.signal2).as_pin_mut().map(|signal| signal.poll_change(waker)) {
            None => true,
            Some(Poll::Ready(None)) => {
                Pin::set(unsafe_pin!(this.signal2), None);
                true
            },
            Some(Poll::Ready(a)) => {
                this.right = a;
                changed = true;
                false
            },
            Some(Poll::Pending) => false,
        };

        let right = match this.right {
            Some(ref mut right) => right,

            None => if right_done {
                return Poll::Ready(None);

            } else {
                return Poll::Pending;
            },
        };

        if changed {
            Poll::Ready(Some((this.callback)(left, right)))

        } else if left_done && right_done {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}


// TODO is it possible to avoid the Arc ?
// TODO is it possible to use only a single Mutex ?
pub type PairMut<A, B> = Arc<(Mutex<Option<A>>, Mutex<Option<B>>)>;

#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct MapPairMut<A, B> where A: Signal, B: Signal {
    signal1: Option<A>,
    signal2: Option<B>,
    inner: PairMut<A::Item, B::Item>,
}

impl<A, B> MapPairMut<A, B>
    where A: Signal,
          B: Signal {
    #[inline]
    pub fn new(left: A, right: B) -> Self {
        Self {
            signal1: Some(left),
            signal2: Some(right),
            inner: Arc::new((Mutex::new(None), Mutex::new(None))),
        }
    }
}

impl<A, B> Unpin for MapPairMut<A, B> where A: Unpin + Signal, B: Unpin + Signal {}

impl<A, B> Signal for MapPairMut<A, B>
    where A: Signal,
          B: Signal {
    type Item = PairMut<A::Item, B::Item>;

    // TODO inline this ?
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        let mut changed = false;

        // TODO is this safe ?
        let this: &mut Self = unsafe_unpin!(self);

        // TODO can this deadlock ?
        let mut borrow_left = this.inner.0.lock().unwrap();

        // TODO is it okay to move this to just above right_done ?
        let mut borrow_right = this.inner.1.lock().unwrap();

        let left_done = match unsafe_pin!(this.signal1).as_pin_mut().map(|signal| signal.poll_change(waker)) {
            None => true,
            Some(Poll::Ready(None)) => {
                Pin::set(unsafe_pin!(this.signal1), None);
                true
            },
            Some(Poll::Ready(a)) => {
                *borrow_left = a;
                changed = true;
                false
            },
            Some(Poll::Pending) => false,
        };

        if borrow_left.is_none() {
            if left_done {
                return Poll::Ready(None);

            } else {
                return Poll::Pending;
            }
        }

        let right_done = match unsafe_pin!(this.signal2).as_pin_mut().map(|signal| signal.poll_change(waker)) {
            None => true,
            Some(Poll::Ready(None)) => {
                Pin::set(unsafe_pin!(this.signal2), None);
                true
            },
            Some(Poll::Ready(a)) => {
                *borrow_right = a;
                changed = true;
                false
            },
            Some(Poll::Pending) => false,
        };

        if borrow_right.is_none() {
            if right_done {
                return Poll::Ready(None);

            } else {
                return Poll::Pending;
            }
        }

        if changed {
            Poll::Ready(Some(this.inner.clone()))

        } else if left_done && right_done {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}


// TODO is it possible to avoid the Arc ?
// TODO maybe it's faster to use a Mutex ?
pub type Pair<A, B> = Arc<RwLock<(Option<A>, Option<B>)>>;

#[derive(Debug)]
#[must_use = "Signals do nothing unless polled"]
pub struct MapPair<A, B> where A: Signal, B: Signal {
    signal1: Option<A>,
    signal2: Option<B>,
    inner: Pair<A::Item, B::Item>,
}

impl<A, B> MapPair<A, B>
    where A: Signal,
          B: Signal {
    #[inline]
    pub fn new(left: A, right: B) -> Self {
        Self {
            signal1: Some(left),
            signal2: Some(right),
            inner: Arc::new(RwLock::new((None, None))),
        }
    }
}

impl<A, B> Unpin for MapPair<A, B> where A: Unpin + Signal, B: Unpin + Signal {}

impl<A, B> Signal for MapPair<A, B>
    where A: Signal,
          B: Signal {
    type Item = Pair<A::Item, B::Item>;

    // TODO inline this ?
    fn poll_change(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Option<Self::Item>> {
        let mut changed = false;

        // TODO is this safe ?
        let this: &mut Self = unsafe_unpin!(self);

        let mut borrow = this.inner.write().unwrap();

        let left_done = match unsafe_pin!(this.signal1).as_pin_mut().map(|signal| signal.poll_change(waker)) {
            None => true,
            Some(Poll::Ready(None)) => {
                Pin::set(unsafe_pin!(this.signal1), None);
                true
            },
            Some(Poll::Ready(a)) => {
                borrow.0 = a;
                changed = true;
                false
            },
            Some(Poll::Pending) => false,
        };

        if borrow.0.is_none() {
            if left_done {
                return Poll::Ready(None);

            } else {
                return Poll::Pending;
            }
        }

        let right_done = match unsafe_pin!(this.signal2).as_pin_mut().map(|signal| signal.poll_change(waker)) {
            None => true,
            Some(Poll::Ready(None)) => {
                Pin::set(unsafe_pin!(this.signal2), None);
                true
            },
            Some(Poll::Ready(a)) => {
                borrow.1 = a;
                changed = true;
                false
            },
            Some(Poll::Pending) => false,
        };

        if borrow.1.is_none() {
            if right_done {
                return Poll::Ready(None);

            } else {
                return Poll::Pending;
            }
        }

        if changed {
            Poll::Ready(Some(this.inner.clone()))

        } else if left_done && right_done {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}
