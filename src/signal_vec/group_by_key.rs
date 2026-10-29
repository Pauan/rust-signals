use std::collections::{BTreeMap, VecDeque};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_channel::mpsc;
use futures_util::stream::StreamExt;
use pin_project::pin_project;

use super::{SignalVec, VecDiff};

/// A group's members, in their source order.
///
/// Poll the outer grouping adapter to drive changes into this stream. Queued
/// changes remain available after the group is removed or the outer stream ends.
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct GroupedVec<A> {
    pub(super) receiver: mpsc::UnboundedReceiver<VecDiff<A>>,
}

impl<A> SignalVec for GroupedVec<A> {
    type Item = A;

    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<A>>> {
        self.get_mut().receiver.poll_next_unpin(cx)
    }
}

#[derive(Debug)]
pub(super) struct Group<A> {
    pub(super) sender: mpsc::UnboundedSender<VecDiff<A>>,
    pub(super) len: usize,
}

impl<A> Group<A> {
    pub(super) fn send(&self, diff: VecDiff<A>) {
        // A dropped consumer does not remove the corresponding source values.
        let _ = self.sender.unbounded_send(diff);
    }
}

/// The sorted groups produced by [`super::SignalVecExt::group_by_key`].
#[pin_project]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct GroupByKey<S, K, F>
where
    S: SignalVec,
{
    #[pin]
    source: Option<S>,
    state: State<S::Item, K, F>,
}

#[derive(Debug)]
struct State<A, K, F> {
    callback: F,
    keys: Vec<K>,
    groups: BTreeMap<K, Group<A>>,
    pending: VecDeque<VecDiff<(K, GroupedVec<A>)>>,
}

impl<S, K, F> GroupByKey<S, K, F>
where
    S: SignalVec,
    K: Ord + Clone,
    F: FnMut(&S::Item) -> K,
{
    pub(super) fn new(source: S, callback: F) -> Self {
        Self {
            source: Some(source),
            state: State {
                callback,
                keys: Vec::new(),
                groups: BTreeMap::new(),
                pending: VecDeque::new(),
            },
        }
    }
}

impl<A, K, F> State<A, K, F>
where
    K: Ord + Clone,
    F: FnMut(&A) -> K,
{
    fn member_index(&self, key: &K, index: usize) -> usize {
        self.keys[..index]
            .iter()
            .filter(|other| *other == key)
            .count()
    }

    fn group_index(&self, key: &K) -> usize {
        self.groups.range(..key).count()
    }

    fn create_group(&mut self, key: K, values: Vec<A>) {
        let index = self.group_index(&key);
        let len = values.len();
        let (sender, receiver) = mpsc::unbounded();
        let group = Group { sender, len };
        group.send(VecDiff::Replace { values });
        self.groups.insert(key.clone(), group);
        self.pending.push_back(VecDiff::InsertAt {
            index,
            value: (key, GroupedVec { receiver }),
        });
    }

    fn retire_group(&mut self, key: &K) {
        let index = self.group_index(key);
        self.groups.remove(key);
        self.pending.push_back(VecDiff::RemoveAt { index });
    }

    fn insert(&mut self, index: usize, value: A) {
        let key = (self.callback)(&value);
        self.insert_keyed(index, key, value);
    }

    fn insert_keyed(&mut self, index: usize, key: K, value: A) {
        let member_index = self.member_index(&key, index);
        self.keys.insert(index, key.clone());
        if let Some(group) = self.groups.get_mut(&key) {
            group.len += 1;
            group.send(VecDiff::InsertAt {
                index: member_index,
                value,
            });
        } else {
            self.create_group(key, vec![value]);
        }
    }

    fn remove(&mut self, index: usize) {
        let key = self.keys.remove(index);
        let member_index = self.member_index(&key, index);
        let group = self.groups.get_mut(&key).unwrap();
        group.len -= 1;
        group.send(VecDiff::RemoveAt {
            index: member_index,
        });
        if group.len == 0 {
            self.retire_group(&key);
        }
    }

    fn update(&mut self, index: usize, value: A) {
        let key = (self.callback)(&value);
        if self.keys[index] == key {
            let member_index = self.member_index(&key, index);
            self.groups[&key].send(VecDiff::UpdateAt {
                index: member_index,
                value,
            });
        } else {
            self.remove(index);
            self.insert_keyed(index, key, value);
        }
    }

    fn move_value(&mut self, old_index: usize, new_index: usize) {
        let key = self.keys.remove(old_index);
        let old_member = self.member_index(&key, old_index);
        let new_member = self.member_index(&key, new_index);
        self.keys.insert(new_index, key.clone());
        if old_member != new_member {
            self.groups[&key].send(VecDiff::Move {
                old_index: old_member,
                new_index: new_member,
            });
        }
    }

    fn replace(&mut self, values: Vec<A>) {
        let mut members = BTreeMap::<K, Vec<A>>::new();
        self.keys.clear();
        for value in values {
            let key = (self.callback)(&value);
            self.keys.push(key.clone());
            members.entry(key).or_default().push(value);
        }

        let missing: Vec<_> = self
            .groups
            .keys()
            .filter(|key| !members.contains_key(*key))
            .cloned()
            .collect();
        for key in missing {
            self.groups[&key].send(VecDiff::Clear {});
            self.retire_group(&key);
        }

        for (key, values) in members {
            if let Some(group) = self.groups.get_mut(&key) {
                group.len = values.len();
                group.send(VecDiff::Replace { values });
            } else {
                self.create_group(key, values);
            }
        }
    }

    fn clear(&mut self) {
        self.keys.clear();
        for group in self.groups.values() {
            group.send(VecDiff::Clear {});
        }
        if !self.groups.is_empty() {
            self.groups.clear();
            self.pending.push_back(VecDiff::Clear {});
        }
    }

    fn apply(&mut self, diff: VecDiff<A>) {
        match diff {
            VecDiff::Replace { values } => self.replace(values),
            VecDiff::InsertAt { index, value } => self.insert(index, value),
            VecDiff::UpdateAt { index, value } => self.update(index, value),
            VecDiff::RemoveAt { index } => self.remove(index),
            VecDiff::Move {
                old_index,
                new_index,
            } => self.move_value(old_index, new_index),
            VecDiff::Push { value } => self.insert(self.keys.len(), value),
            VecDiff::Pop {} => self.remove(self.keys.len() - 1),
            VecDiff::Clear {} => self.clear(),
        }
    }
}

impl<S, K, F> SignalVec for GroupByKey<S, K, F>
where
    S: SignalVec,
    K: Ord + Clone,
    F: FnMut(&S::Item) -> K,
{
    type Item = (K, GroupedVec<S::Item>);

    fn poll_vec_change(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<VecDiff<Self::Item>>> {
        let mut this = self.project();
        loop {
            if let Some(diff) = this.state.pending.pop_front() {
                return Poll::Ready(Some(diff));
            }
            match this.source.as_mut().as_pin_mut() {
                Some(source) => match source.poll_vec_change(cx) {
                    Poll::Ready(Some(diff)) => this.state.apply(diff),
                    Poll::Ready(None) => {
                        this.source.set(None);
                        this.state.groups.clear();
                        this.state.keys.clear();
                    }
                    Poll::Pending => return Poll::Pending,
                },
                None => return Poll::Ready(None),
            }
        }
    }
}
