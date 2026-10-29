use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_channel::mpsc;
use pin_project::pin_project;

use super::group_by_key::{Group, GroupedVec};
use super::{SignalVec, VecDiff};
use crate::signal::Signal;

#[derive(Debug)]
struct Member<A, S, K> {
    value: A,
    signal: Option<Pin<Box<S>>>,
    keys: BTreeSet<K>,
}

#[derive(Debug)]
struct State<A, S, K, F> {
    callback: F,
    members: Vec<Member<A, S, K>>,
    groups: BTreeMap<K, Group<A>>,
    pending: VecDeque<VecDiff<(K, GroupedVec<A>)>>,
}

/// Groups produced by [`super::SignalVecExt::group_by_keys_signal`].
#[pin_project]
#[derive(Debug)]
#[must_use = "SignalVecs do nothing unless polled"]
pub struct GroupByKeysSignal<A: SignalVec, S, K, F> {
    #[pin]
    source: Option<A>,
    state: State<A::Item, S, K, F>,
}

impl<A: SignalVec, S, K, F> GroupByKeysSignal<A, S, K, F> {
    pub(super) fn new(source: A, callback: F) -> Self {
        Self {
            source: Some(source),
            state: State {
                callback,
                members: Vec::new(),
                groups: BTreeMap::new(),
                pending: VecDeque::new(),
            },
        }
    }
}

impl<A, S, K, F> State<A, S, K, F>
where
    A: Clone,
    K: Ord + Clone,
    S: Signal<Item = Vec<K>>,
    F: FnMut(&A) -> S,
{
    fn member(&mut self, value: A, cx: &mut Context) -> Member<A, S, K> {
        let mut signal = Box::pin((self.callback)(&value));
        let keys = super::unwrap(signal.as_mut().poll_change(cx))
            .into_iter()
            .collect();
        Member {
            value,
            signal: Some(signal),
            keys,
        }
    }

    fn rank(&self, key: &K, index: usize) -> usize {
        self.members[..index]
            .iter()
            .filter(|member| member.keys.contains(key))
            .count()
    }

    fn create_group(&mut self, key: K, values: Vec<A>) {
        let index = self.groups.range(..&key).count();
        let (sender, receiver) = mpsc::unbounded();
        let group = Group {
            sender,
            len: values.len(),
        };
        group.send(VecDiff::Replace { values });
        self.groups.insert(key.clone(), group);
        self.pending.push_back(VecDiff::InsertAt {
            index,
            value: (key, GroupedVec { receiver }),
        });
    }

    fn retire_group(&mut self, key: &K) {
        let index = self.groups.range(..key).count();
        self.groups.remove(key);
        self.pending.push_back(VecDiff::RemoveAt { index });
    }

    fn insert_membership(&mut self, key: K, index: usize) {
        let rank = self.rank(&key, index);
        let value = self.members[index].value.clone();
        if let Some(group) = self.groups.get_mut(&key) {
            group.len += 1;
            group.send(VecDiff::InsertAt { index: rank, value });
        } else {
            self.create_group(key, vec![value]);
        }
    }

    fn remove_membership(&mut self, key: &K, index: usize) {
        let rank = self.rank(key, index);
        let group = self.groups.get_mut(key).unwrap();
        group.send(VecDiff::RemoveAt { index: rank });
        group.len -= 1;
        if group.len == 0 {
            self.retire_group(key);
        }
    }

    fn reconcile(&mut self, index: usize, old_keys: BTreeSet<K>, updated: bool) {
        let keys = self.members[index].keys.clone();
        for key in old_keys.difference(&keys) {
            self.remove_membership(key, index);
        }
        for key in keys.difference(&old_keys) {
            self.insert_membership(key.clone(), index);
        }
        if updated {
            for key in keys.intersection(&old_keys) {
                self.groups[key].send(VecDiff::UpdateAt {
                    index: self.rank(key, index),
                    value: self.members[index].value.clone(),
                });
            }
        }
    }

    fn insert(&mut self, index: usize, value: A, cx: &mut Context) {
        let member = self.member(value, cx);
        let keys = member.keys.clone();
        self.members.insert(index, member);
        for key in keys {
            self.insert_membership(key, index);
        }
    }

    fn update(&mut self, index: usize, value: A, cx: &mut Context) {
        self.members[index].signal = None;
        let member = self.member(value, cx);
        let old = std::mem::replace(&mut self.members[index], member);
        self.reconcile(index, old.keys, true);
    }

    fn remove(&mut self, index: usize) {
        let old = self.members.remove(index);
        for key in old.keys {
            self.remove_membership(&key, index);
        }
    }

    fn move_member(&mut self, old_index: usize, new_index: usize) {
        let old_ranks: Vec<_> = self.members[old_index]
            .keys
            .iter()
            .map(|key| (key.clone(), self.rank(key, old_index)))
            .collect();
        let member = self.members.remove(old_index);
        self.members.insert(new_index, member);
        for (key, old_index) in old_ranks {
            let new_index = self.rank(&key, new_index);
            if old_index != new_index {
                self.groups[&key].send(VecDiff::Move {
                    old_index,
                    new_index,
                });
            }
        }
    }

    fn replace(&mut self, values: Vec<A>, cx: &mut Context) {
        self.members.clear();
        let mut memberships = BTreeMap::<K, Vec<A>>::new();
        for value in values {
            let member = self.member(value, cx);
            for key in &member.keys {
                memberships
                    .entry(key.clone())
                    .or_default()
                    .push(member.value.clone());
            }
            self.members.push(member);
        }
        let missing: Vec<_> = self
            .groups
            .keys()
            .filter(|key| !memberships.contains_key(*key))
            .cloned()
            .collect();
        for key in missing {
            self.groups[&key].send(VecDiff::Clear {});
            self.retire_group(&key);
        }
        for (key, values) in memberships {
            if let Some(group) = self.groups.get_mut(&key) {
                group.len = values.len();
                group.send(VecDiff::Replace { values });
            } else {
                self.create_group(key, values);
            }
        }
    }

    fn clear(&mut self) {
        self.members.clear();
        for group in self.groups.values() {
            group.send(VecDiff::Clear {});
        }
        if !self.groups.is_empty() {
            self.groups.clear();
            self.pending.push_back(VecDiff::Clear {});
        }
    }

    fn apply(&mut self, diff: VecDiff<A>, cx: &mut Context) {
        match diff {
            VecDiff::Replace { values } => self.replace(values, cx),
            VecDiff::InsertAt { index, value } => self.insert(index, value, cx),
            VecDiff::Push { value } => self.insert(self.members.len(), value, cx),
            VecDiff::UpdateAt { index, value } => self.update(index, value, cx),
            VecDiff::RemoveAt { index } => self.remove(index),
            VecDiff::Pop {} => self.remove(self.members.len() - 1),
            VecDiff::Move {
                old_index,
                new_index,
            } => self.move_member(old_index, new_index),
            VecDiff::Clear {} => self.clear(),
        }
    }

    fn poll_keys(&mut self, cx: &mut Context) -> bool {
        let mut active = false;
        for index in 0..self.members.len() {
            let member = &mut self.members[index];
            match member
                .signal
                .as_mut()
                .map(|signal| signal.as_mut().poll_change(cx))
            {
                Some(Poll::Ready(Some(keys))) => {
                    active = true;
                    let old = std::mem::replace(&mut member.keys, keys.into_iter().collect());
                    self.reconcile(index, old, false);
                    // Member-only changes may leave no outer diff to drive the
                    // next poll, including one that observes signal completion.
                    cx.waker().wake_by_ref();
                }
                Some(Poll::Ready(None)) => member.signal = None,
                Some(Poll::Pending) => active = true,
                None => {}
            }
        }
        active
    }
}

impl<A, S, K, F> SignalVec for GroupByKeysSignal<A, S, K, F>
where
    A: SignalVec,
    A::Item: Clone,
    K: Ord + Clone,
    S: Signal<Item = Vec<K>>,
    F: FnMut(&A::Item) -> S,
{
    type Item = (K, GroupedVec<A::Item>);

    fn poll_vec_change(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<VecDiff<Self::Item>>> {
        let mut this = self.project();
        if let Some(diff) = this.state.pending.pop_front() {
            return Poll::Ready(Some(diff));
        }
        while let Some(source) = this.source.as_mut().as_pin_mut() {
            match source.poll_vec_change(cx) {
                Poll::Ready(Some(diff)) => this.state.apply(diff, cx),
                Poll::Ready(None) => this.source.set(None),
                Poll::Pending => break,
            }
        }
        let active = this.state.poll_keys(cx);
        if this.source.is_none() && !active {
            this.state.groups.clear();
            this.state.members.clear();
        }
        if let Some(diff) = this.state.pending.pop_front() {
            Poll::Ready(Some(diff))
        } else if this.source.is_none() && !active {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}
