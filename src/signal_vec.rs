use std::marker::PhantomData;
use std::cmp::Ordering;
use futures_core::task::Context;
use futures_core::{Future, Stream, Poll, Async, Never};
use futures_core::future::IntoFuture;
use futures_util::stream;
use futures_util::stream::StreamExt;
use signal::Signal;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VecChange<A> {
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
        changes: Vec<VecChange<A>>,
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

impl<A> VecChange<A> {
    // TODO inline this ?
    fn map<B, F>(self, mut callback: F) -> VecChange<B> where F: FnMut(A) -> B {
        match self {
            // TODO figure out a more efficient way of implementing this
            VecChange::Replace { values } => VecChange::Replace { values: values.into_iter().map(callback).collect() },
            VecChange::InsertAt { index, value } => VecChange::InsertAt { index, value: callback(value) },
            VecChange::UpdateAt { index, value } => VecChange::UpdateAt { index, value: callback(value) },
            VecChange::Push { value } => VecChange::Push { value: callback(value) },
            VecChange::RemoveAt { index } => VecChange::RemoveAt { index },
            VecChange::Move { old_index, new_index } => VecChange::Move { old_index, new_index },
            VecChange::Pop {} => VecChange::Pop {},
            VecChange::Clear {} => VecChange::Clear {},
        }
    }
}


pub trait IntoSignalVec {
    type SignalVec: SignalVec<Item = Self::Item>;
    type Item;

    fn into_signal_vec(self) -> Self::SignalVec;
}

impl<A> IntoSignalVec for A where A: SignalVec {
    type SignalVec = Self;
    type Item = A::Item;

    fn into_signal_vec(self) -> Self { self }
}


pub trait SignalVec {
    type Item;

    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecChange<Self::Item>>>;
}

impl<F: ?Sized + SignalVec> SignalVec for ::std::boxed::Box<F> {
    type Item = F::Item;

    #[inline]
    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecChange<Self::Item>>> {
        (**self).poll_vec_change(cx)
    }
}


pub trait SignalVecExt: SignalVec {
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
            callback,
        }
    }

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
    fn to_stream(self) -> SignalVecStream<Self, Never> where Self: Sized {
        SignalVecStream {
            signal: self,
            phantom: PhantomData,
        }
    }

    #[inline]
    // TODO file Rust bug about bad error message when `callback` isn't marked as `mut`
    fn for_each<U, F>(self, callback: F) -> ForEach<Self, U, F>
        where U: IntoFuture<Item = ()>,
              F: FnMut(VecChange<Self::Item>) -> U,
              Self:Sized {
        // TODO a little hacky
        ForEach {
            inner: SignalVecStream {
                signal: self,
                phantom: PhantomData,
            }.for_each(callback)
        }
    }

    #[inline]
    fn len(self) -> Len<Self> where Self: Sized {
        Len {
            signal: Some(self),
            first: true,
            len: 0,
        }
    }

    #[inline]
    fn by_ref(&mut self) -> &mut Self {
        self
    }
}

// TODO why is this ?Sized
impl<T: ?Sized> SignalVecExt for T where T: SignalVec {}


pub struct ForEach<A, B, C> where B: IntoFuture {
    inner: stream::ForEach<SignalVecStream<A, B::Error>, B, C>
}

impl<A, B, C> Future for ForEach<A, B, C>
    where A: SignalVec,
          B: IntoFuture<Item = ()>,
          C: FnMut(VecChange<A::Item>) -> B {
    type Item = ();
    type Error = B::Error;

    #[inline]
    fn poll(&mut self, cx: &mut Context) -> Poll<Self::Item, Self::Error> {
        // TODO a teensy bit hacky
        self.inner.poll(cx).map(|async| async.map(|_| ()))
    }
}


pub struct Map<A, B> {
    signal: A,
    callback: B,
}

impl<A, B, F> SignalVec for Map<A, F>
    where A: SignalVec,
          F: FnMut(A::Item) -> B {
    type Item = B;

    // TODO should this inline ?
    #[inline]
    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecChange<Self::Item>>> {
        self.signal.poll_vec_change(cx).map(|some| some.map(|change| change.map(|value| (self.callback)(value))))
    }
}


fn unwrap<A>(x: Async<Option<A>>) -> A {
    match x {
        Async::Ready(Some(x)) => x,
        _ => panic!("Signal did not return a value"),
    }
}

pub struct MapSignal<A, B: Signal, F> {
    signal: Option<A>,
    signals: Vec<Option<B>>,
    callback: F,
}

impl<A, B, F> SignalVec for MapSignal<A, B, F>
    where A: SignalVec,
          B: Signal,
          F: FnMut(A::Item) -> B {
    type Item = B::Item;

    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecChange<Self::Item>>> {
        let done = match self.signal.as_mut().map(|signal| signal.poll_vec_change(cx)) {
            None => true,
            Some(Async::Ready(None)) => {
                self.signal = None;
                true
            },
            Some(Async::Ready(Some(change))) => {
                return Async::Ready(Some(match change {
                    VecChange::Replace { values } => {
                        self.signals = Vec::with_capacity(values.len());

                        VecChange::Replace {
                            values: values.into_iter().map(|value| {
                                let mut signal = (self.callback)(value);
                                let poll = unwrap(signal.poll_change(cx));
                                self.signals.push(Some(signal));
                                poll
                            }).collect()
                        }
                    },

                    VecChange::InsertAt { index, value } => {
                        let mut signal = (self.callback)(value);
                        let poll = unwrap(signal.poll_change(cx));
                        self.signals.insert(index, Some(signal));
                        VecChange::InsertAt { index, value: poll }
                    },

                    VecChange::UpdateAt { index, value } => {
                        let mut signal = (self.callback)(value);
                        let poll = unwrap(signal.poll_change(cx));
                        self.signals[index] = Some(signal);
                        VecChange::UpdateAt { index, value: poll }
                    },

                    VecChange::Push { value } => {
                        let mut signal = (self.callback)(value);
                        let poll = unwrap(signal.poll_change(cx));
                        self.signals.push(Some(signal));
                        VecChange::Push { value: poll }
                    },

                    VecChange::Move { old_index, new_index } => {
                        let value = self.signals.remove(old_index);
                        self.signals.insert(new_index, value);
                        VecChange::Move { old_index, new_index }
                    },

                    VecChange::RemoveAt { index } => {
                        self.signals.remove(index);
                        VecChange::RemoveAt { index }
                    },

                    VecChange::Pop {} => {
                        self.signals.pop().unwrap();
                        VecChange::Pop {}
                    },

                    VecChange::Clear {} => {
                        self.signals.clear();
                        VecChange::Clear {}
                    },
                }));
            },
            Some(Async::Pending) => false,
        };

        // TODO make this more efficient (e.g. using a similar strategy as FuturesUnordered)
        let mut iter = self.signals.as_mut_slice().into_iter().enumerate();

        let mut has_pending = false;

        // TODO ensure that this is as efficient as possible
        // TODO make this more efficient (e.g. using a similar strategy as FuturesUnordered)
        loop {
            match iter.next() {
                Some((index, signal)) => match signal.as_mut().map(|s| s.poll_change(cx)) {
                    Some(Async::Ready(Some(value))) => {
                        return Async::Ready(Some(VecChange::UpdateAt { index, value }))
                    },
                    Some(Async::Ready(None)) => {
                        *signal = None;
                    },
                    Some(Async::Pending) => {
                        has_pending = true;
                    },
                    None => {},
                },
                None => return if done && !has_pending {
                    Async::Ready(None)

                } else {
                    Async::Pending
                },
            }
        }
    }
}


pub struct Len<A> {
    signal: Option<A>,
    first: bool,
    len: usize,
}

impl<A> Signal for Len<A> where A: SignalVec {
    type Item = usize;

    fn poll_change(&mut self, cx: &mut Context) -> Async<Option<Self::Item>> {
        let mut changed = false;
        let mut done = false;

        loop {
            match self.signal.as_mut().map(|signal| signal.poll_vec_change(cx)) {
                None => {
                    done = true;
                    break;
                },
                Some(Async::Ready(None)) => {
                    self.signal = None;
                    done = true;
                    break;
                },
                Some(Async::Ready(Some(change))) => match change {
                    VecChange::Replace { values } => {
                        let len = values.len();

                        if self.len != len {
                            self.len = len;
                            changed = true;
                        }
                    },

                    VecChange::InsertAt { .. } | VecChange::Push { .. } => {
                        self.len += 1;
                        changed = true;
                    },

                    VecChange::UpdateAt { .. } | VecChange::Move { .. } => {},

                    VecChange::RemoveAt { .. } | VecChange::Pop {} => {
                        self.len -= 1;
                        changed = true;
                    },

                    VecChange::Clear {} => {
                        if self.len != 0 {
                            self.len = 0;
                            changed = true;
                        }
                    },
                },
                Some(Async::Pending) => {
                    break;
                },
            }
        }

        if changed || self.first {
            self.first = false;
            Async::Ready(Some(self.len))

        } else if done {
            Async::Ready(None)

        } else {
            Async::Pending
        }
    }
}


pub struct SignalVecStream<A, Error> {
    signal: A,
    phantom: PhantomData<Error>,
}

impl<A: SignalVec, Error> Stream for SignalVecStream<A, Error> {
    type Item = VecChange<A::Item>;
    type Error = Error;

    #[inline]
    fn poll_next(&mut self, cx: &mut Context) -> Poll<Option<Self::Item>, Self::Error> {
        Ok(self.signal.poll_vec_change(cx))
    }
}


pub struct Filter<A, B> {
    // TODO use a bit vec for smaller size
    indexes: Vec<bool>,
    signal: A,
    callback: B,
}

impl<A, B> Filter<A, B> {
    fn find_index(&self, index: usize) -> usize {
        self.indexes[0..index].into_iter().filter(|x| **x).count()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.indexes.iter().filter(|x| **x).count()
    }
}

impl<A, F> SignalVec for Filter<A, F>
    where A: SignalVec,
          F: FnMut(&A::Item) -> bool {
    type Item = A::Item;

    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecChange<Self::Item>>> {
        loop {
            return match self.signal.poll_vec_change(cx) {
                Async::Pending => Async::Pending,
                Async::Ready(None) => Async::Ready(None),
                Async::Ready(Some(change)) => match change {
                    VecChange::Replace { values } => {
                        self.indexes = Vec::with_capacity(values.len());

                        Async::Ready(Some(VecChange::Replace {
                            values: values.into_iter().filter(|value| {
                                let keep = (self.callback)(value);
                                self.indexes.push(keep);
                                keep
                            }).collect()
                        }))
                    },

                    VecChange::InsertAt { index, value } => {
                        if (self.callback)(&value) {
                            self.indexes.insert(index, true);
                            Async::Ready(Some(VecChange::InsertAt { index: self.find_index(index), value }))

                        } else {
                            self.indexes.insert(index, false);
                            continue;
                        }
                    },

                    VecChange::UpdateAt { index, value } => {
                        if (self.callback)(&value) {
                            if self.indexes[index] {
                                Async::Ready(Some(VecChange::UpdateAt { index: self.find_index(index), value }))

                            } else {
                                self.indexes[index] = true;
                                Async::Ready(Some(VecChange::InsertAt { index: self.find_index(index), value }))
                            }

                        } else {
                            if self.indexes[index] {
                                self.indexes[index] = false;
                                Async::Ready(Some(VecChange::RemoveAt { index: self.find_index(index) }))

                            } else {
                                continue;
                            }
                        }
                    },

                    // TODO unit tests for this
                    VecChange::Move { old_index, new_index } => {
                        if self.indexes.remove(old_index) {
                            self.indexes.insert(new_index, true);

                            Async::Ready(Some(VecChange::Move {
                                old_index: self.find_index(old_index),
                                new_index: self.find_index(new_index),
                            }))

                        } else {
                            self.indexes.insert(new_index, false);
                            continue;
                        }
                    },

                    VecChange::RemoveAt { index } => {
                        if self.indexes.remove(index) {
                            Async::Ready(Some(VecChange::RemoveAt { index: self.find_index(index) }))

                        } else {
                            continue;
                        }
                    },

                    VecChange::Push { value } => {
                        if (self.callback)(&value) {
                            self.indexes.push(true);
                            Async::Ready(Some(VecChange::Push { value }))

                        } else {
                            self.indexes.push(false);
                            continue;
                        }
                    },

                    VecChange::Pop {} => {
                        if self.indexes.pop().expect("Cannot pop from empty vec") {
                            Async::Ready(Some(VecChange::Pop {}))

                        } else {
                            continue;
                        }
                    },

                    VecChange::Clear {} => {
                        self.indexes.clear();
                        Async::Ready(Some(VecChange::Clear {}))
                    },
                },
            }
        }
    }
}


pub struct SortByCloned<A: SignalVec, B> {
    pending: Option<Async<Option<VecChange<A::Item>>>>,
    values: Vec<A::Item>,
    indexes: Vec<usize>,
    signal: A,
    compare: B,
}

impl<A, F> SortByCloned<A, F>
    where A: SignalVec,
          F: FnMut(&A::Item, &A::Item) -> Ordering {
    // TODO should this inline ?
    fn binary_search(&mut self, index: usize) -> Result<usize, usize> {
        let compare = &mut self.compare;
        let values = &self.values;
        let value = &values[index];

        // TODO use get_unchecked ?
        self.indexes.binary_search_by(|i| compare(&values[*i], value).then_with(|| i.cmp(&index)))
    }

    fn binary_search_insert(&mut self, index: usize) -> usize {
        match self.binary_search(index) {
            Ok(_) => panic!("Value already exists"),
            Err(new_index) => new_index,
        }
    }

    fn binary_search_remove(&mut self, index: usize) -> usize {
        self.binary_search(index).expect("Could not find value")
    }

    fn increment_indexes(&mut self, start: usize) {
        for index in &mut self.indexes {
            let i = *index;

            if i >= start {
                *index = i + 1;
            }
        }
    }

    fn decrement_indexes(&mut self, start: usize) {
        for index in &mut self.indexes {
            let i = *index;

            if i > start {
                *index = i - 1;
            }
        }
    }

    fn insert_at(&mut self, sorted_index: usize, index: usize, value: A::Item) -> Async<Option<VecChange<A::Item>>> {
        if sorted_index == self.indexes.len() {
            self.indexes.push(index);

            Async::Ready(Some(VecChange::Push {
                value,
            }))

        } else {
            self.indexes.insert(sorted_index, index);

            Async::Ready(Some(VecChange::InsertAt {
                index: sorted_index,
                value,
            }))
        }
    }

    fn remove_at(&mut self, sorted_index: usize) -> Async<Option<VecChange<A::Item>>> {
        if sorted_index == (self.indexes.len() - 1) {
            self.indexes.pop();

            Async::Ready(Some(VecChange::Pop {}))

        } else {
            self.indexes.remove(sorted_index);

            Async::Ready(Some(VecChange::RemoveAt {
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
    fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecChange<Self::Item>>> {
        match self.pending.take() {
            Some(value) => value,
            None => loop {
                return match self.signal.poll_vec_change(cx) {
                    Async::Pending => Async::Pending,
                    Async::Ready(None) => Async::Ready(None),
                    Async::Ready(Some(change)) => match change {
                        VecChange::Replace { mut values } => {
                            // TODO can this be made faster ?
                            let mut indexes: Vec<usize> = (0..values.len()).collect();

                            // TODO use get_unchecked ?
                            indexes.sort_unstable_by(|a, b| (self.compare)(&values[*a], &values[*b]).then_with(|| a.cmp(b)));

                            let output = Async::Ready(Some(VecChange::Replace {
                                // TODO use get_unchecked ?
                                values: indexes.iter().map(|i| values[*i].clone()).collect()
                            }));

                            self.values = values;
                            self.indexes = indexes;

                            output
                        },

                        VecChange::InsertAt { index, value } => {
                            let new_value = value.clone();

                            self.values.insert(index, value);

                            self.increment_indexes(index);

                            let sorted_index = self.binary_search_insert(index);

                            self.insert_at(sorted_index, index, new_value)
                        },

                        VecChange::Push { value } => {
                            let new_value = value.clone();

                            let index = self.values.len();

                            self.values.push(value);

                            let sorted_index = self.binary_search_insert(index);

                            self.insert_at(sorted_index, index, new_value)
                        },

                        VecChange::UpdateAt { index, value } => {
                            let old_index = self.binary_search_remove(index);

                            let old_output = self.remove_at(old_index);

                            let new_value = value.clone();

                            self.values[index] = value;

                            let new_index = self.binary_search_insert(index);

                            if old_index == new_index {
                                self.indexes.insert(new_index, index);

                                Async::Ready(Some(VecChange::UpdateAt {
                                    index: new_index,
                                    value: new_value,
                                }))

                            } else {
                                let new_output = self.insert_at(new_index, index, new_value);
                                self.pending = Some(new_output);

                                old_output
                            }
                        },

                        VecChange::RemoveAt { index } => {
                            let sorted_index = self.binary_search_remove(index);

                            self.values.remove(index);

                            self.decrement_indexes(index);

                            self.remove_at(sorted_index)
                        },

                        // TODO can this be made more efficient ?
                        VecChange::Move { old_index, new_index } => {
                            let old_sorted_index = self.binary_search_remove(old_index);

                            let value = self.values.remove(old_index);

                            self.decrement_indexes(old_index);

                            self.indexes.remove(old_sorted_index);

                            self.values.insert(new_index, value);

                            self.increment_indexes(new_index);

                            let new_sorted_index = self.binary_search_insert(new_index);

                            self.indexes.insert(new_sorted_index, new_index);

                            if old_sorted_index == new_sorted_index {
                                continue;

                            } else {
                                Async::Ready(Some(VecChange::Move {
                                    old_index: old_sorted_index,
                                    new_index: new_sorted_index,
                                }))
                            }
                        },

                        VecChange::Pop {} => {
                            let index = self.values.len() - 1;

                            let sorted_index = self.binary_search_remove(index);

                            self.values.pop();

                            self.remove_at(sorted_index)
                        },

                        VecChange::Clear {} => {
                            self.values.clear();
                            self.indexes.clear();
                            Async::Ready(Some(VecChange::Clear {}))
                        },
                    },
                }
            },
        }
    }
}


// TODO verify that this is correct
mod mutable_vec {
    use super::{SignalVec, VecChange};
    use std::sync::{Arc, RwLock};
    use futures_channel::mpsc;
    use futures_core::{Async, Stream};
    use futures_core::task::Context;
    use serde::{Serialize, Deserialize, Serializer, Deserializer};


    struct MutableVecState<A> {
        values: Vec<A>,
        senders: Vec<mpsc::UnboundedSender<VecChange<A>>>,
    }

    impl<A> MutableVecState<A> {
        // TODO should this inline ?
        #[inline]
        fn notify<B: FnMut() -> VecChange<A>>(&mut self, mut change: B) {
            self.senders.retain(|sender| {
                sender.unbounded_send(change()).is_ok()
            });
        }

        fn notify_with<B, C, D, E>(&mut self, value: B, mut clone: C, change: D, mut notify: E)
            where C: FnMut(&B) -> B,
                  D: FnOnce(&mut Self, B),
                  E: FnMut(B) -> VecChange<A> {

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
                self.notify(|| VecChange::Pop {});
            }

            value
        }

        fn remove(&mut self, index: usize) -> A {
            let len = self.values.len();

            let value = self.values.remove(index);

            if index == (len - 1) {
                self.notify(|| VecChange::Pop {});

            } else {
                self.notify(|| VecChange::RemoveAt { index });
            }

            value
        }

        fn move_from_to(&mut self, old_index: usize, new_index: usize) {
            let value = self.values.remove(old_index);
            self.values.insert(new_index, value);
            self.notify(|| VecChange::Move { old_index, new_index });
        }

        fn clear(&mut self) {
            if self.values.len() > 0 {
                self.values.clear();

                self.notify(|| VecChange::Clear {});
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
                    self.notify(|| VecChange::Clear {});

                } else {
                    // TODO use VecChange::Batch
                    for index in removals.into_iter().rev() {
                        len -= 1;

                        if index == len {
                            self.notify(|| VecChange::Pop {});

                        } else {
                            self.notify(|| VecChange::RemoveAt { index });
                        }
                    }
                }
            }
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
                sender.unbounded_send(VecChange::Replace { values: Self::copy_values(&self.values) }).unwrap();
            }

            self.senders.push(sender);

            MutableSignalVec {
                receiver
            }
        }

        fn push_copy(&mut self, value: A) {
            self.values.push(value);
            self.notify(|| VecChange::Push { value });
        }

        fn insert_copy(&mut self, index: usize, value: A) {
            if index == self.values.len() {
                self.push_copy(value);

            } else {
                self.values.insert(index, value);
                self.notify(|| VecChange::InsertAt { index, value });
            }
        }

        fn set_copy(&mut self, index: usize, value: A) {
            self.values[index] = value;
            self.notify(|| VecChange::UpdateAt { index, value });
        }

        fn replace_copy(&mut self, values: Vec<A>) {
            self.notify_with(values,
                Self::copy_values,
                |this, values| this.values = values,
                |values| VecChange::Replace { values });
        }
    }

    impl<A: Clone> MutableVecState<A> {
        #[inline]
        fn notify_clone<B, C, D>(&mut self, value: B, change: C, notify: D)
            where B: Clone,
                  C: FnOnce(&mut Self, B),
                  D: FnMut(B) -> VecChange<A> {

            self.notify_with(value, |a| a.clone(), change, notify)
        }

        // TODO change this to return a MutableSignalVecClone ?
        fn signal_vec_clone(&mut self) -> MutableSignalVec<A> {
            let (sender, receiver) = mpsc::unbounded();

            if self.values.len() > 0 {
                sender.unbounded_send(VecChange::Replace { values: self.values.clone() }).unwrap();
            }

            self.senders.push(sender);

            MutableSignalVec {
                receiver
            }
        }

        fn push_clone(&mut self, value: A) {
            self.notify_clone(value,
                |this, value| this.values.push(value),
                |value| VecChange::Push { value });
        }

        fn insert_clone(&mut self, index: usize, value: A) {
            if index == self.values.len() {
                self.push_clone(value);

            } else {
                self.notify_clone(value,
                    |this, value| this.values.insert(index, value),
                    |value| VecChange::InsertAt { index, value });
            }
        }

        fn set_clone(&mut self, index: usize, value: A) {
            self.notify_clone(value,
                |this, value| this.values[index] = value,
                |value| VecChange::UpdateAt { index, value });
        }

        fn replace_clone(&mut self, values: Vec<A>) {
            self.notify_clone(values,
                |this, values| this.values = values,
                |values| VecChange::Replace { values });
        }
    }


    // TODO get rid of the Arc
    pub struct MutableVec<A>(Arc<RwLock<MutableVecState<A>>>);

    impl<A> MutableVec<A> {
        #[inline]
        pub fn new_with_values(values: Vec<A>) -> Self {
            MutableVec(Arc::new(RwLock::new(MutableVecState {
                values,
                senders: vec![],
            })))
        }

        #[inline]
        pub fn new() -> Self {
            Self::new_with_values(vec![])
        }

        #[inline]
        pub fn pop(&self) -> Option<A> {
            self.0.write().unwrap().pop()
        }

        #[inline]
        pub fn remove(&self, index: usize) -> A {
            self.0.write().unwrap().remove(index)
        }

        #[inline]
        pub fn clear(&self) {
            self.0.write().unwrap().clear()
        }

        #[inline]
        pub fn move_from_to(&self, old_index: usize, new_index: usize) {
            self.0.write().unwrap().move_from_to(old_index, new_index);
        }

        #[inline]
        pub fn retain<F>(&self, f: F) where F: FnMut(&A) -> bool {
            self.0.write().unwrap().retain(f)
        }

        pub fn with_slice<B, F>(&self, f: F) -> B where F: FnOnce(&[A]) -> B {
            let lock = self.0.read().unwrap();
            f(&lock.values)
        }

        #[inline]
        pub fn len(&self) -> usize {
            self.0.read().unwrap().values.len()
        }

        #[inline]
        pub fn is_empty(&self) -> bool {
            self.0.read().unwrap().values.is_empty()
        }
    }

    impl<A: Copy> MutableVec<A> {
        #[inline]
        pub fn signal_vec(&self) -> MutableSignalVec<A> {
            self.0.write().unwrap().signal_vec_copy()
        }

        #[inline]
        pub fn push(&self, value: A) {
            self.0.write().unwrap().push_copy(value)
        }

        #[inline]
        pub fn insert(&self, index: usize, value: A) {
            self.0.write().unwrap().insert_copy(index, value)
        }

        // TODO replace this with something else, like entry or IndexMut or whatever
        #[inline]
        pub fn set(&self, index: usize, value: A) {
            self.0.write().unwrap().set_copy(index, value)
        }

        #[inline]
        pub fn replace(&self, values: Vec<A>) {
            self.0.write().unwrap().replace_copy(values)
        }
    }

    impl<A: Clone> MutableVec<A> {
        #[inline]
        pub fn signal_vec_cloned(&self) -> MutableSignalVec<A> {
            self.0.write().unwrap().signal_vec_clone()
        }

        #[inline]
        pub fn push_cloned(&self, value: A) {
            self.0.write().unwrap().push_clone(value)
        }

        #[inline]
        pub fn insert_cloned(&self, index: usize, value: A) {
            self.0.write().unwrap().insert_clone(index, value)
        }

        // TODO replace this with something else, like entry or IndexMut or whatever
        #[inline]
        pub fn set_cloned(&self, index: usize, value: A) {
            self.0.write().unwrap().set_clone(index, value)
        }

        #[inline]
        pub fn replace_cloned(&self, values: Vec<A>) {
            self.0.write().unwrap().replace_clone(values)
        }
    }

    impl<T> Serialize for MutableVec<T> where T: Serialize {
        #[inline]
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
            self.0.read().unwrap().values.serialize(serializer)
        }
    }

    impl<'de, T> Deserialize<'de> for MutableVec<T> where T: Deserialize<'de> {
        #[inline]
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de> {
            <Vec<T>>::deserialize(deserializer).map(MutableVec::new_with_values)
        }
    }

    impl<T> Default for MutableVec<T> {
        #[inline]
        fn default() -> Self {
            MutableVec::new()
        }
    }


    pub struct MutableSignalVec<A> {
        receiver: mpsc::UnboundedReceiver<VecChange<A>>,
    }

    impl<A> SignalVec for MutableSignalVec<A> {
        type Item = A;

        #[inline]
        fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecChange<Self::Item>>> {
            self.receiver.poll_next(cx).unwrap()
        }
    }
}

pub use self::mutable_vec::*;


#[cfg(test)]
mod tests {
    use futures_core::{Future, Poll};
    use futures_executor::block_on;
    use super::*;
    use super::mutable_vec::MutableVec;


    struct Tester<A> {
        changes: Vec<Async<VecChange<A>>>,
    }

    impl<A> Tester<A> {
        #[inline]
        fn new(changes: Vec<Async<VecChange<A>>>) -> Self {
            Self { changes }
        }
    }

    impl<A> SignalVec for Tester<A> {
        type Item = A;

        #[inline]
        fn poll_vec_change(&mut self, cx: &mut Context) -> Async<Option<VecChange<Self::Item>>> {
            if self.changes.len() > 0 {
                match self.changes.remove(0) {
                    Async::Pending => {
                        cx.waker().wake();
                        Async::Pending
                    },
                    Async::Ready(change) => Async::Ready(Some(change)),
                }

            } else {
                Async::Ready(None)
            }
        }
    }


    struct TesterFuture<A, B> {
        signal: A,
        callback: B,
    }

    impl<A: SignalVec, B: FnMut(&mut A, VecChange<A::Item>)> TesterFuture<A, B> {
        #[inline]
        fn new(signal: A, callback: B) -> Self {
            Self { signal, callback }
        }
    }

    impl<A, B> Future for TesterFuture<A, B>
        where A: SignalVec,
              B: FnMut(&mut A, VecChange<A::Item>) {

        type Item = ();
        type Error = Never;

        #[inline]
        fn poll(&mut self, cx: &mut Context) -> Poll<Self::Item, Self::Error> {
            loop {
                return match self.signal.poll_vec_change(cx) {
                    Async::Ready(Some(change)) => {
                        (self.callback)(&mut self.signal, change);
                        continue;
                    },
                    Async::Ready(None) => Ok(Async::Ready(())),
                    Async::Pending => Ok(Async::Pending),
                }
            }
        }
    }

    fn run<A: SignalVec, B, C: FnMut(&mut A, VecChange<A::Item>) -> B>(signal: A, mut callback: C) -> Vec<B> {
        let mut changes = vec![];

        block_on(TesterFuture::new(signal, |signal, change| {
            changes.push(callback(signal, change));
        })).unwrap();

        changes
    }


    #[test]
    fn send_sync() {
        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new());
        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new().signal_vec());
        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new().signal_vec_cloned());

        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new_with_values(vec![]));
        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new_with_values(vec![]).signal_vec());
        let _: Box<Send + Sync> = Box::new(MutableVec::<()>::new_with_values(vec![]).signal_vec_cloned());
    }

    #[test]
    fn filter() {
        #[derive(Debug, PartialEq, Eq)]
        struct Change {
            length: usize,
            indexes: Vec<bool>,
            change: VecChange<u32>,
        }

        let input = Tester::new(vec![
            Async::Ready(VecChange::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
            Async::Pending,
            Async::Ready(VecChange::InsertAt { index: 0, value: 6 }),
            Async::Ready(VecChange::InsertAt { index: 2, value: 7 }),
            Async::Pending,
            Async::Pending,
            Async::Pending,
            Async::Ready(VecChange::InsertAt { index: 5, value: 8 }),
            Async::Ready(VecChange::InsertAt { index: 7, value: 9 }),
            Async::Ready(VecChange::InsertAt { index: 9, value: 10 }),
            Async::Pending,
            Async::Ready(VecChange::InsertAt { index: 11, value: 11 }),
            Async::Pending,
            Async::Ready(VecChange::InsertAt { index: 0, value: 0 }),
            Async::Pending,
            Async::Pending,
            Async::Ready(VecChange::InsertAt { index: 1, value: 0 }),
            Async::Ready(VecChange::InsertAt { index: 5, value: 0 }),
            Async::Pending,
            Async::Ready(VecChange::InsertAt { index: 5, value: 12 }),
            Async::Pending,
            Async::Ready(VecChange::RemoveAt { index: 0 }),
            Async::Ready(VecChange::RemoveAt { index: 0 }),
            Async::Pending,
            Async::Ready(VecChange::RemoveAt { index: 0 }),
            Async::Ready(VecChange::RemoveAt { index: 1 }),
            Async::Pending,
            Async::Ready(VecChange::RemoveAt { index: 0 }),
            Async::Pending,
            Async::Ready(VecChange::RemoveAt { index: 0 }),
        ]);

        let output = input.filter(|&x| x == 3 || x == 4 || x > 5);

        assert_eq!(Filter::len(&output), 0);
        assert_eq!(output.indexes, vec![]);

        let changes = run(output, |output, change| {
            Change {
                change: change,
                length: Filter::len(&output),
                indexes: output.indexes.clone(),
            }
        });

        assert_eq!(changes, vec![
            Change { length: 2, indexes: vec![false, false, false, true, true, false], change: VecChange::Replace { values: vec![3, 4] } },
            Change { length: 3, indexes: vec![true, false, false, false, true, true, false], change: VecChange::InsertAt { index: 0, value: 6 } },
            Change { length: 4, indexes: vec![true, false, true, false, false, true, true, false], change: VecChange::InsertAt { index: 1, value: 7 } },
            Change { length: 5, indexes: vec![true, false, true, false, false, true, true, true, false], change: VecChange::InsertAt { index: 2, value: 8 } },
            Change { length: 6, indexes: vec![true, false, true, false, false, true, true, true, true, false], change: VecChange::InsertAt { index: 4, value: 9 } },
            Change { length: 7, indexes: vec![true, false, true, false, false, true, true, true, true, true, false], change: VecChange::InsertAt { index: 6, value: 10 } },
            Change { length: 8, indexes: vec![true, false, true, false, false, true, true, true, true, true, false, true], change: VecChange::InsertAt { index: 7, value: 11 } },
            Change { length: 9, indexes: vec![false, false, true, false, true, true, false, false, false, true, true, true, true, true, false, true], change: VecChange::InsertAt { index: 2, value: 12 } },
            Change { length: 8, indexes: vec![false, true, true, false, false, false, true, true, true, true, true, false, true], change: VecChange::RemoveAt { index: 0 } },
            Change { length: 7, indexes: vec![false, true, false, false, false, true, true, true, true, true, false, true], change: VecChange::RemoveAt { index: 0 } },
            Change { length: 6, indexes: vec![false, false, false, true, true, true, true, true, false, true], change: VecChange::RemoveAt { index: 0 } },
        ]);
    }
}
