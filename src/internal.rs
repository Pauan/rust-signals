use super::signal::{State, Signal};
use std::rc::Rc;
use std::cell::RefCell;


pub fn unwrap_mut<A>(x: &mut Option<A>) -> &mut A {
    match *x {
        Some(ref mut x) => x,
        None => unreachable!(),
    }
}


pub struct Map2<A: Signal, B: Signal, C> {
    signal1: A,
    signal2: B,
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
            signal1: left,
            signal2: right,
            callback,
            left: None,
            right: None,
        }
    }
}

impl<A, B, C, D> Signal for Map2<A, B, C>
    where A: Signal,
          B: Signal,
          C: FnMut(&mut A::Item, &mut B::Item) -> D {
    type Item = D;

    // TODO inline this ?
    fn poll(&mut self) -> State<Self::Item> {
        let mut changed = false;

        if let State::Changed(left) = self.signal1.poll() {
            self.left = Some(left);
            changed = true;
        }

        let left = match self.left {
            Some(ref mut left) => left,
            None => return State::NotChanged,
        };

        if let State::Changed(right) = self.signal2.poll() {
            self.right = Some(right);
            changed = true;
        }

        let right = match self.right {
            Some(ref mut right) => right,
            None => return State::NotChanged,
        };

        if changed {
            State::Changed((self.callback)(left, right))

        } else {
            State::NotChanged
        }
    }
}


// TODO is it possible to avoid the Rc ?
pub type PairMut<A, B> = Rc<(RefCell<Option<A>>, RefCell<Option<B>>)>;

pub struct MapPairMut<A: Signal, B: Signal> {
    signal1: A,
    signal2: B,
    inner: PairMut<A::Item, B::Item>,
}

impl<A, B> MapPairMut<A, B>
    where A: Signal,
          B: Signal {
    #[inline]
    pub fn new(left: A, right: B) -> Self {
        Self {
            signal1: left,
            signal2: right,
            inner: Rc::new((RefCell::new(None), RefCell::new(None))),
        }
    }
}

impl<A, B> Signal for MapPairMut<A, B>
    where A: Signal,
          B: Signal {
    type Item = PairMut<A::Item, B::Item>;

    // TODO inline this ?
    fn poll(&mut self) -> State<Self::Item> {
        let mut changed = false;

        let mut borrow_left = self.inner.0.borrow_mut();

        if let State::Changed(left) = self.signal1.poll() {
            *borrow_left = Some(left);
            changed = true;
        }

        if borrow_left.is_none() {
            return State::NotChanged;
        }

        let mut borrow_right = self.inner.1.borrow_mut();

        if let State::Changed(right) = self.signal2.poll() {
            *borrow_right = Some(right);
            changed = true;
        }

        if borrow_right.is_none() {
            return State::NotChanged;
        }

        if changed {
            State::Changed(self.inner.clone())

        } else {
            State::NotChanged
        }
    }
}
