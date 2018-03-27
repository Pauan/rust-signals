use super::signal::Signal;
use std::rc::Rc;
use std::cell::RefCell;
use futures::Async;


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


pub struct Map2<A: Signal, B: Signal, C> {
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

// Poll left  => Has left   => Poll right  => Has right   => Output
// -----------------------------------------------------------------------------
// Some(left) =>            => Some(right) =>             => Some((left, right))
// Some(left) =>            => None        => Some(right) => Some((left, right))
// Some(left) =>            => NotReady    => Some(right) => Some((left, right))
// None       => Some(left) => Some(right) =>             => Some((left, right))
// NotReady   => Some(left) => Some(right) =>             => Some((left, right))
// None       => Some(left) => None        => Some(right) => None
// None       => None       =>             =>             => None
//            =>            => None        => None        => None
// Some(left) =>            => NotReady    => None        => NotReady
// None       => Some(left) => NotReady    => Some(right) => NotReady
// None       => Some(left) => NotReady    => None        => NotReady
// NotReady   => Some(left) => None        => Some(right) => NotReady
// NotReady   => Some(left) => NotReady    => Some(right) => NotReady
// NotReady   => Some(left) => NotReady    => None        => NotReady
// NotReady   => None       => Some(right) =>             => NotReady
// NotReady   => None       => None        => Some(right) => NotReady
// NotReady   => None       => NotReady    => Some(right) => NotReady
// NotReady   => None       => NotReady    => None        => NotReady
impl<A, B, C, D> Signal for Map2<A, B, C>
    where A: Signal,
          B: Signal,
          C: FnMut(&mut A::Item, &mut B::Item) -> D {
    type Item = D;

    // TODO inline this ?
    fn poll(&mut self) -> Async<Option<Self::Item>> {
        let mut changed = false;

        let left_done = match self.signal1.as_mut().map(|signal| signal.poll()) {
            None => true,
            Some(Async::Ready(None)) => {
                self.signal1 = None;
                true
            },
            Some(Async::Ready(a)) => {
                self.left = a;
                changed = true;
                false
            },
            Some(Async::NotReady) => false,
        };

        let left = match self.left {
            Some(ref mut left) => left,

            None => if left_done {
                return Async::Ready(None);

            } else {
                return Async::NotReady;
            },
        };

        let right_done = match self.signal2.as_mut().map(|signal| signal.poll()) {
            None => true,
            Some(Async::Ready(None)) => {
                self.signal2 = None;
                true
            },
            Some(Async::Ready(a)) => {
                self.right = a;
                changed = true;
                false
            },
            Some(Async::NotReady) => false,
        };

        let right = match self.right {
            Some(ref mut right) => right,

            None => if right_done {
                return Async::Ready(None);

            } else {
                return Async::NotReady;
            },
        };

        if changed {
            Async::Ready(Some((self.callback)(left, right)))

        } else if left_done && right_done {
            Async::Ready(None)

        } else {
            Async::NotReady
        }
    }
}


// TODO is it possible to avoid the Rc ?
pub type PairMut<A, B> = Rc<(RefCell<Option<A>>, RefCell<Option<B>>)>;

pub struct MapPairMut<A: Signal, B: Signal> {
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
            inner: Rc::new((RefCell::new(None), RefCell::new(None))),
        }
    }
}

impl<A, B> Signal for MapPairMut<A, B>
    where A: Signal,
          B: Signal {
    type Item = PairMut<A::Item, B::Item>;

    // TODO inline this ?
    fn poll(&mut self) -> Async<Option<Self::Item>> {
        let mut changed = false;

        let mut borrow_left = self.inner.0.borrow_mut();

        let left_done = match self.signal1.as_mut().map(|signal| signal.poll()) {
            None => true,
            Some(Async::Ready(None)) => {
                self.signal1 = None;
                true
            },
            Some(Async::Ready(a)) => {
                *borrow_left = a;
                changed = true;
                false
            },
            Some(Async::NotReady) => false,
        };

        if borrow_left.is_none() {
            if left_done {
                return Async::Ready(None);

            } else {
                return Async::NotReady;
            }
        }

        let mut borrow_right = self.inner.1.borrow_mut();

        let right_done = match self.signal2.as_mut().map(|signal| signal.poll()) {
            None => true,
            Some(Async::Ready(None)) => {
                self.signal2 = None;
                true
            },
            Some(Async::Ready(a)) => {
                *borrow_right = a;
                changed = true;
                false
            },
            Some(Async::NotReady) => false,
        };

        if borrow_right.is_none() {
            if right_done {
                return Async::Ready(None);

            } else {
                return Async::NotReady;
            }
        }

        if changed {
            Async::Ready(Some(self.inner.clone()))

        } else if left_done && right_done {
            Async::Ready(None)

        } else {
            Async::NotReady
        }
    }
}


// TODO is it possible to avoid the Rc ?
pub type Pair<A, B> = Rc<RefCell<(Option<A>, Option<B>)>>;

pub struct MapPair<A: Signal, B: Signal> {
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
            inner: Rc::new(RefCell::new((None, None))),
        }
    }
}

impl<A, B> Signal for MapPair<A, B>
    where A: Signal,
          B: Signal {
    type Item = Pair<A::Item, B::Item>;

    // TODO inline this ?
    fn poll(&mut self) -> Async<Option<Self::Item>> {
        let mut changed = false;

        let mut borrow = self.inner.borrow_mut();

        let left_done = match self.signal1.as_mut().map(|signal| signal.poll()) {
            None => true,
            Some(Async::Ready(None)) => {
                self.signal1 = None;
                true
            },
            Some(Async::Ready(a)) => {
                borrow.0 = a;
                changed = true;
                false
            },
            Some(Async::NotReady) => false,
        };

        if borrow.0.is_none() {
            if left_done {
                return Async::Ready(None);

            } else {
                return Async::NotReady;
            }
        }

        let right_done = match self.signal2.as_mut().map(|signal| signal.poll()) {
            None => true,
            Some(Async::Ready(None)) => {
                self.signal2 = None;
                true
            },
            Some(Async::Ready(a)) => {
                borrow.1 = a;
                changed = true;
                false
            },
            Some(Async::NotReady) => false,
        };

        if borrow.1.is_none() {
            if right_done {
                return Async::Ready(None);

            } else {
                return Async::NotReady;
            }
        }

        if changed {
            Async::Ready(Some(self.inner.clone()))

        } else if left_done && right_done {
            Async::Ready(None)

        } else {
            Async::NotReady
        }
    }
}
