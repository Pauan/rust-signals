use futures_signals::signal::Signal;
use futures_signals::signal_vec::{Always, SignalVec, SignalVecExt, VecDiff};
use futures_util::task::{noop_waker, waker, ArcWake};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::ops::Deref;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::task::{Context, Poll, Waker};

#[derive(Default)]
struct Keys {
    value: Vec<i32>,
    version: usize,
    ended: bool,
    polls: usize,
    drops: usize,
    wakes: Vec<Waker>,
}
#[derive(Clone)]
struct Item {
    id: i32,
    keys: Rc<RefCell<Keys>>,
}
fn item(id: i32, keys: &[i32]) -> Item {
    Item {
        id,
        keys: Rc::new(RefCell::new(Keys {
            value: keys.to_vec(),
            ..Keys::default()
        })),
    }
}
impl Item {
    fn set(&self, value: &[i32]) {
        let wakes = {
            let mut keys = self.keys.borrow_mut();
            assert!(!keys.ended);
            keys.version += 1;
            keys.value = value.to_vec();
            std::mem::take(&mut keys.wakes)
        };
        for wake in wakes {
            wake.wake();
        }
    }
    fn end(&self) {
        let wakes = {
            let mut k = self.keys.borrow_mut();
            k.ended = true;
            std::mem::take(&mut k.wakes)
        };
        for wake in wakes {
            wake.wake();
        }
    }
}
struct KeySignal {
    keys: Rc<RefCell<Keys>>,
    version: Option<usize>,
    ended: bool,
}
impl Signal for KeySignal {
    type Item = Vec<i32>;
    fn poll_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        assert!(!self.ended, "completed key signal was polled again");
        let result = {
            let mut keys = self.keys.borrow_mut();
            keys.polls += 1;
            keys.wakes.push(cx.waker().clone());
            if self.version != Some(keys.version) {
                Some((keys.version, keys.value.clone()))
            } else {
                None
            }
        };
        if let Some((version, value)) = result {
            self.version = Some(version);
            Poll::Ready(Some(value))
        } else if self.keys.borrow().ended {
            self.ended = true;
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}
impl Drop for KeySignal {
    fn drop(&mut self) {
        self.keys.borrow_mut().drops += 1;
    }
}

#[derive(Default)]
struct Feed {
    changes: VecDeque<VecDiff<Item>>,
    ended: bool,
    polls: usize,
    drops: usize,
    wake: Option<Waker>,
}
struct Input(Rc<RefCell<Feed>>);
impl Drop for Input {
    fn drop(&mut self) {
        self.0.borrow_mut().drops += 1;
    }
}
impl SignalVec for Input {
    type Item = Item;
    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Item>>> {
        let mut f = self.0.borrow_mut();
        f.polls += 1;
        if let Some(diff) = f.changes.pop_front() {
            Poll::Ready(Some(diff))
        } else if f.ended {
            Poll::Ready(None)
        } else {
            f.wake = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
struct MissingGrouping;
impl MissingGrouping {
    #[allow(dead_code)]
    #[track_caller]
    fn group_by_keys_signal<K, S, F>(&self, _: F) -> Always<(K, Always<Item>)>
    where
        S: Signal<Item = Vec<K>>,
        F: FnMut(&Item) -> S,
    {
        panic!("SignalVecExt::group_by_keys_signal is unavailable at this revision")
    }
}
impl Deref for Input {
    type Target = MissingGrouping;
    fn deref(&self) -> &Self::Target {
        &MissingGrouping
    }
}

type Child = Pin<Box<dyn SignalVec<Item = (i32, usize)>>>;
struct ChildState {
    stream: Option<Child>,
    values: Vec<(i32, usize)>,
    history: Vec<Vec<i32>>,
    ended: bool,
}
struct Harness {
    feed: Rc<RefCell<Feed>>,
    outer: Option<Pin<Box<dyn SignalVec<Item = (i32, usize)>>>>,
    groups: Rc<RefCell<BTreeMap<usize, ChildState>>>,
    visible: Vec<(i32, usize)>,
    factory: Rc<RefCell<Vec<i32>>>,
    ended: bool,
}
impl Harness {
    fn new() -> Self {
        let feed = Rc::new(RefCell::new(Feed::default()));
        let factory = Rc::new(RefCell::new(Vec::new()));
        let calls = factory.clone();
        let groups = Rc::new(RefCell::new(BTreeMap::new()));
        let observed = groups.clone();
        let next = Cell::new(0);
        let member_next = Rc::new(Cell::new(0));
        let outer = Input(feed.clone())
            .group_by_keys_signal(move |item: &Item| {
                calls.borrow_mut().push(item.id);
                KeySignal {
                    keys: item.keys.clone(),
                    version: None,
                    ended: false,
                }
            })
            .map(move |(key, child)| {
                let id = next.get();
                next.set(id + 1);
                let serial = member_next.clone();
                let stream = child.map(move |item| {
                    let id = serial.get();
                    serial.set(id + 1);
                    (item.id, id)
                });
                observed.borrow_mut().insert(
                    id,
                    ChildState {
                        stream: Some(Box::pin(stream)),
                        values: vec![],
                        history: vec![],
                        ended: false,
                    },
                );
                (key, id)
            });
        Self {
            feed,
            outer: Some(Box::pin(outer)),
            groups,
            visible: vec![],
            factory,
            ended: false,
        }
    }
    fn send(&mut self, diff: VecDiff<Item>) {
        let wake = {
            let mut f = self.feed.borrow_mut();
            f.changes.push_back(diff);
            f.wake.take()
        };
        if let Some(wake) = wake {
            wake.wake();
        }
    }
    fn end_source(&mut self) {
        let wake = {
            let mut f = self.feed.borrow_mut();
            f.ended = true;
            f.wake.take()
        };
        if let Some(wake) = wake {
            wake.wake();
        }
    }
    fn pump_with(&mut self, wake: &Waker) {
        assert!(!self.ended, "test attempted to poll completed outer stream");
        let relay = Arc::new(Relay {
            wake: wake.clone(),
            notified: AtomicBool::new(false),
        });
        let forwarded = waker(relay.clone());
        let mut cx = Context::from_waker(&forwarded);
        for _ in 0..10000 {
            relay.notified.store(false, Ordering::SeqCst);
            match self
                .outer
                .as_mut()
                .unwrap()
                .as_mut()
                .poll_vec_change(&mut cx)
            {
                Poll::Ready(Some(diff)) => diff.apply_to_vec(&mut self.visible),
                Poll::Ready(None) => {
                    self.ended = true;
                    return;
                }
                Poll::Pending => {
                    if !relay.notified.swap(false, Ordering::SeqCst) {
                        return;
                    }
                }
            }
        }
        panic!("unbounded notifications without new inputs");
    }
    fn pump(&mut self) {
        self.pump_with(&noop_waker());
    }
    fn drain_with(&self, id: usize, wake: &Waker) {
        let mut groups = self.groups.borrow_mut();
        let g = groups.get_mut(&id).unwrap();
        if g.ended || g.stream.is_none() {
            return;
        }
        let mut cx = Context::from_waker(wake);
        loop {
            match g.stream.as_mut().unwrap().as_mut().poll_vec_change(&mut cx) {
                Poll::Ready(Some(diff)) => {
                    diff.apply_to_vec(&mut g.values);
                    g.history.push(g.values.iter().map(|v| v.0).collect());
                }
                Poll::Ready(None) => {
                    g.ended = true;
                    return;
                }
                Poll::Pending => return,
            }
        }
    }
    fn drain(&self, id: usize) {
        self.drain_with(id, &noop_waker());
    }
    fn sync(&mut self) {
        self.pump();
        let ids: Vec<_> = self.groups.borrow().keys().copied().collect();
        for id in ids {
            self.drain(id);
        }
    }
    fn seed(&mut self, values: Vec<Item>) {
        self.send(VecDiff::Replace { values });
        self.sync();
    }
    fn id(&self, key: i32) -> usize {
        self.visible
            .iter()
            .find(|v| v.0 == key)
            .expect("present key")
            .1
    }
    fn tokens(&self, key: i32) -> Vec<(i32, usize)> {
        self.groups.borrow()[&self.id(key)].values.clone()
    }
    fn snapshot(&self) -> Vec<(i32, Vec<i32>)> {
        self.visible
            .iter()
            .map(|(k, id)| {
                (
                    *k,
                    self.groups.borrow()[id]
                        .values
                        .iter()
                        .map(|v| v.0)
                        .collect(),
                )
            })
            .collect()
    }
}
#[derive(Default)]
struct Awoken(AtomicBool);
impl ArcWake for Awoken {
    fn wake_by_ref(this: &Arc<Self>) {
        this.0.store(true, Ordering::SeqCst);
    }
}
struct Relay {
    wake: Waker,
    notified: AtomicBool,
}
impl ArcWake for Relay {
    fn wake_by_ref(this: &Arc<Self>) {
        this.notified.store(true, Ordering::SeqCst);
        this.wake.wake_by_ref();
    }
}

#[test]
fn initial_overlaps_are_sets_but_equal_source_positions_stay_distinct() {
    let same = item(7, &[3, 1, 3, 1]);
    let mut h = Harness::new();
    h.seed(vec![same.clone(), item(8, &[]), same]);
    assert_eq!(h.snapshot(), vec![(1, vec![7, 7]), (3, vec![7, 7])]);
    assert_eq!(&*h.factory.borrow(), &[7, 8, 7]);
}

#[test]
fn key_changes_preserve_intersections_and_other_consumers() {
    let a = item(10, &[1, 2]);
    let b = item(20, &[2, 3]);
    let c = item(30, &[1, 3]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b, c]);
    let two = h.tokens(2);
    let three = h.tokens(3);
    let groups = h.visible.clone();
    a.set(&[3, 2, 2]);
    h.sync();
    assert_eq!(
        h.snapshot(),
        vec![(1, vec![30]), (2, vec![10, 20]), (3, vec![10, 20, 30])]
    );
    assert_eq!(h.tokens(2), two);
    assert_eq!(&h.tokens(3)[1..], &three);
    assert_eq!(h.visible, groups);
    assert_eq!(&*h.factory.borrow(), &[10, 20, 30]);
}

#[test]
fn reordered_and_duplicate_key_notifications_do_not_recreate_members() {
    let a = item(1, &[2, 1]);
    let mut h = Harness::new();
    h.seed(vec![a.clone()]);
    let one = h.tokens(1);
    let two = h.tokens(2);
    let groups = h.visible.clone();
    for keys in [vec![1, 2], vec![2, 2, 1, 1], vec![1, 2, 1]] {
        a.set(&keys);
        h.sync();
    }
    assert_eq!(h.tokens(1), one);
    assert_eq!(h.tokens(2), two);
    assert_eq!(h.visible, groups);
}

#[test]
fn empty_membership_retirement_and_reappearance_use_current_source_position() {
    let a = item(1, &[4]);
    let b = item(2, &[]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b.clone()]);
    let old = h.id(4);
    a.set(&[]);
    h.sync();
    assert!(h.visible.is_empty());
    assert!(h.groups.borrow()[&old].ended);
    assert!(h.groups.borrow()[&old].values.is_empty());
    h.send(VecDiff::Move {
        old_index: 1,
        new_index: 0,
    });
    h.sync();
    a.set(&[4]);
    b.set(&[4]);
    h.sync();
    assert_ne!(h.id(4), old);
    assert_eq!(h.snapshot(), vec![(4, vec![2, 1])]);
    assert_eq!(&*h.factory.borrow(), &[1, 2]);
}

#[test]
fn moves_preserve_each_membership_and_subscription() {
    let a = item(1, &[1, 2]);
    let b = item(2, &[2]);
    let c = item(3, &[1, 2]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b, c]);
    let one = h.tokens(1);
    let two = h.tokens(2);
    h.send(VecDiff::Move {
        old_index: 0,
        new_index: 2,
    });
    h.sync();
    assert_eq!(h.tokens(1), vec![one[1], one[0]]);
    assert_eq!(h.tokens(2), vec![two[1], two[2], two[0]]);
    h.send(VecDiff::Move {
        old_index: 2,
        new_index: 0,
    });
    h.sync();
    h.send(VecDiff::Move {
        old_index: 0,
        new_index: 0,
    });
    h.sync();
    assert_eq!(h.tokens(1), one);
    assert_eq!(h.tokens(2), two);
    a.set(&[2, 3]);
    h.sync();
    assert_eq!(
        h.snapshot(),
        vec![(1, vec![3]), (2, vec![1, 2, 3]), (3, vec![1])]
    );
    assert_eq!(a.keys.borrow().drops, 0);
    assert_eq!(&*h.factory.borrow(), &[1, 2, 3]);
}

#[test]
fn insertion_and_removal_translate_ranks_across_hidden_items() {
    let a = item(1, &[1, 2]);
    let b = item(2, &[]);
    let c = item(3, &[2]);
    let mut h = Harness::new();
    h.seed(vec![a, b.clone(), c]);
    h.send(VecDiff::InsertAt {
        index: 0,
        value: item(4, &[1, 2]),
    });
    h.send(VecDiff::InsertAt {
        index: 4,
        value: item(5, &[2]),
    });
    h.sync();
    b.set(&[1, 2]);
    h.sync();
    assert_eq!(
        h.snapshot(),
        vec![(1, vec![4, 1, 2]), (2, vec![4, 1, 2, 3, 5])]
    );
    h.send(VecDiff::RemoveAt { index: 1 });
    h.send(VecDiff::Pop {});
    h.sync();
    assert_eq!(h.snapshot(), vec![(1, vec![4, 2]), (2, vec![4, 2, 3])]);
}

#[test]
fn source_update_replaces_value_in_common_groups_and_cancels_old_signal() {
    let a = item(1, &[1, 2]);
    let b = item(2, &[2]);
    let replacement = item(8, &[2, 3]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b]);
    let group = h.id(2);
    let unchanged = h.tokens(2)[1];
    let old_polls = a.keys.borrow().polls;
    a.set(&[99]);
    h.send(VecDiff::UpdateAt {
        index: 0,
        value: replacement.clone(),
    });
    h.sync();
    assert_eq!(h.snapshot(), vec![(2, vec![8, 2]), (3, vec![8])]);
    assert_eq!(h.id(2), group);
    assert_eq!(h.tokens(2)[1], unchanged);
    assert_eq!(a.keys.borrow().drops, 1);
    assert_eq!(a.keys.borrow().polls, old_polls);
    replacement.set(&[3]);
    h.sync();
    assert_eq!(h.snapshot(), vec![(2, vec![2]), (3, vec![8])]);
}

#[test]
fn sole_member_update_keeps_the_common_group_alive() {
    let mut h = Harness::new();
    h.seed(vec![item(1, &[5, 6])]);
    let id = h.id(5);
    h.send(VecDiff::UpdateAt {
        index: 0,
        value: item(2, &[5, 7]),
    });
    h.sync();
    assert_eq!(h.id(5), id);
    assert!(!h.groups.borrow()[&id].ended);
    assert_eq!(h.snapshot(), vec![(5, vec![2]), (7, vec![2])]);
}

#[test]
fn source_replace_preserves_common_groups_and_restarts_all_subscriptions() {
    let a = item(1, &[1, 2]);
    let b = item(2, &[2, 3]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b.clone()]);
    let two = h.id(2);
    let three = h.id(3);
    let one = h.id(1);
    a.set(&[9]);
    b.set(&[10]);
    let fresh = item(7, &[3, 2, 2]);
    h.send(VecDiff::Replace {
        values: vec![fresh.clone(), item(8, &[2, 4])],
    });
    h.sync();
    assert_eq!(h.id(2), two);
    assert_eq!(h.id(3), three);
    assert!(h.groups.borrow()[&one].ended);
    assert_eq!(
        h.snapshot(),
        vec![(2, vec![7, 8]), (3, vec![7]), (4, vec![8])]
    );
    assert_eq!(a.keys.borrow().drops, 1);
    assert_eq!(b.keys.borrow().drops, 1);
    fresh.set(&[4]);
    h.sync();
    assert_eq!(h.snapshot(), vec![(2, vec![8]), (4, vec![7, 8])]);
}

#[test]
fn clear_cancels_hidden_and_visible_subscriptions_and_allows_reuse() {
    let a = item(1, &[1]);
    let b = item(2, &[]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b.clone()]);
    let old = h.id(1);
    a.set(&[9]);
    b.set(&[10]);
    h.send(VecDiff::Clear {});
    h.sync();
    assert!(h.visible.is_empty());
    assert!(h.groups.borrow()[&old].ended);
    assert!(h.groups.borrow()[&old].values.is_empty());
    assert_eq!(a.keys.borrow().drops, 1);
    assert_eq!(b.keys.borrow().drops, 1);
    h.send(VecDiff::Push {
        value: item(3, &[1]),
    });
    h.sync();
    assert_ne!(h.id(1), old);
}

#[test]
fn empty_replace_and_removing_hidden_items_cancel_key_signals() {
    let a = item(1, &[]);
    let b = item(2, &[2]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b.clone()]);
    a.set(&[5]);
    h.send(VecDiff::RemoveAt { index: 0 });
    h.sync();
    assert_eq!(h.snapshot(), vec![(2, vec![2])]);
    assert_eq!(a.keys.borrow().drops, 1);
    let old = h.id(2);
    b.set(&[9]);
    h.send(VecDiff::Replace { values: vec![] });
    h.sync();
    assert!(h.visible.is_empty());
    assert!(h.groups.borrow()[&old].ended);
    assert_eq!(b.keys.borrow().drops, 1);
}

#[test]
fn source_end_waits_for_live_signals_and_keeps_their_changes_deliverable() {
    let a = item(1, &[1]);
    let b = item(2, &[]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b.clone()]);
    h.end_source();
    h.sync();
    assert!(!h.ended);
    a.set(&[2]);
    b.set(&[2, 3]);
    h.sync();
    assert_eq!(h.snapshot(), vec![(2, vec![1, 2]), (3, vec![2])]);
    a.end();
    h.sync();
    assert!(!h.ended);
    b.set(&[3]);
    b.end();
    h.sync();
    assert!(h.ended);
    assert_eq!(h.snapshot(), vec![(2, vec![1]), (3, vec![2])]);
    for id in h.visible.iter().map(|v| v.1) {
        assert!(h.groups.borrow()[&id].ended);
    }
}

#[test]
fn ended_key_signals_freeze_memberships_and_are_not_repolled() {
    let a = item(1, &[1, 2]);
    let b = item(2, &[]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b.clone()]);
    a.end();
    b.end();
    h.sync();
    assert!(!h.ended);
    assert_eq!(a.keys.borrow().drops, 1);
    assert_eq!(b.keys.borrow().drops, 1);
    h.send(VecDiff::Move {
        old_index: 0,
        new_index: 1,
    });
    h.sync();
    h.send(VecDiff::Push {
        value: item(3, &[2]),
    });
    h.sync();
    assert_eq!(h.snapshot(), vec![(1, vec![1]), (2, vec![1, 3])]);
}

#[test]
fn ready_source_removal_wins_over_a_key_notification() {
    let a = item(1, &[1]);
    let mut h = Harness::new();
    h.seed(vec![a.clone()]);
    let polls = a.keys.borrow().polls;
    a.set(&[99]);
    h.send(VecDiff::Pop {});
    h.sync();
    assert!(h.visible.is_empty());
    assert_eq!(h.groups.borrow().len(), 1);
    assert_eq!(a.keys.borrow().polls, polls);
    assert_eq!(a.keys.borrow().drops, 1);
}

#[test]
fn latest_outer_waker_is_notified_by_a_key_change() {
    let a = item(1, &[1]);
    let mut h = Harness::new();
    h.seed(vec![a.clone()]);
    let old = Arc::new(Awoken::default());
    let latest = Arc::new(Awoken::default());
    h.pump_with(&waker(old));
    h.pump_with(&waker(latest.clone()));
    a.set(&[2]);
    assert!(latest.0.load(Ordering::SeqCst));
    h.sync();
    assert_eq!(h.snapshot(), vec![(2, vec![1])]);
}

#[test]
fn inner_only_updates_wake_waiting_member_consumers() {
    let a = item(1, &[1, 2]);
    let b = item(2, &[2]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b]);
    let two = h.id(2);
    let awake = Arc::new(Awoken::default());
    h.drain_with(two, &waker(awake.clone()));
    a.set(&[1]);
    h.pump();
    assert!(awake.0.load(Ordering::SeqCst));
    h.drain(two);
    assert_eq!(h.snapshot(), vec![(1, vec![1]), (2, vec![2])]);
}

#[test]
fn construction_and_member_polling_do_not_drive_key_signals() {
    let a = item(1, &[1]);
    let mut h = Harness::new();
    h.send(VecDiff::Push { value: a.clone() });
    assert!(h.factory.borrow().is_empty());
    assert_eq!(a.keys.borrow().polls, 0);
    h.sync();
    let id = h.id(1);
    let polls = a.keys.borrow().polls;
    let source_polls = h.feed.borrow().polls;
    a.set(&[2]);
    h.drain(id);
    h.drain(id);
    assert_eq!(a.keys.borrow().polls, polls);
    assert_eq!(h.feed.borrow().polls, source_polls);
    assert_eq!(h.snapshot(), vec![(1, vec![1])]);
    h.sync();
    assert_eq!(h.snapshot(), vec![(2, vec![1])]);
}

#[test]
fn dropping_one_membership_consumer_does_not_cancel_its_item() {
    let a = item(1, &[1, 2]);
    let b = item(2, &[2]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b]);
    let one = h.id(1);
    h.groups.borrow_mut().get_mut(&one).unwrap().stream = None;
    a.set(&[1, 3]);
    h.sync();
    assert_eq!(h.id(1), one);
    assert_eq!(h.tokens(2).iter().map(|v| v.0).collect::<Vec<_>>(), vec![2]);
    assert_eq!(h.tokens(3).iter().map(|v| v.0).collect::<Vec<_>>(), vec![1]);
    assert_eq!(a.keys.borrow().drops, 0);
    a.set(&[2]);
    h.sync();
    assert_eq!(h.snapshot(), vec![(2, vec![1, 2])]);
    a.set(&[1, 2]);
    h.sync();
    assert_ne!(h.id(1), one);
}

#[test]
fn lagging_members_drain_each_processed_key_change_before_retirement() {
    let a = item(1, &[1]);
    let b = item(2, &[]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b.clone()]);
    let id = h.id(1);
    b.set(&[1]);
    h.pump();
    a.set(&[]);
    h.pump();
    b.set(&[]);
    h.pump();
    assert!(h.visible.is_empty());
    h.drain(id);
    let groups = h.groups.borrow();
    let g = &groups[&id];
    assert!(g.ended);
    let mut position = 0;
    for expected in [vec![1, 2], vec![2], vec![]] {
        let offset = g.history[position..]
            .iter()
            .position(|state| state == &expected)
            .unwrap_or_else(|| panic!("missing ordered state {:?}: {:?}", expected, g.history));
        position += offset + 1;
    }
}

#[test]
fn outer_drop_cancels_all_signals_but_drains_queued_memberships() {
    let a = item(1, &[1]);
    let b = item(2, &[]);
    let c = item(3, &[2]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b.clone(), c.clone()]);
    let id = h.id(1);
    let untouched = h.id(2);
    b.set(&[1]);
    h.pump();
    let awake = Arc::new(Awoken::default());
    h.drain(id);
    a.set(&[]);
    h.pump();
    h.drain_with(untouched, &waker(awake.clone()));
    awake.0.store(false, Ordering::SeqCst);
    let polls = a.keys.borrow().polls;
    a.set(&[99]);
    h.outer = None;
    assert_eq!(a.keys.borrow().polls, polls);
    assert_eq!(a.keys.borrow().drops, 1);
    assert_eq!(b.keys.borrow().drops, 1);
    assert_eq!(c.keys.borrow().drops, 1);
    assert_eq!(h.feed.borrow().drops, 1);
    assert!(awake.0.load(Ordering::SeqCst));
    h.drain(id);
    assert_eq!(
        h.groups.borrow()[&id]
            .values
            .iter()
            .map(|v| v.0)
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert!(h.groups.borrow()[&id].ended);
}

#[test]
fn factory_accounting_survives_composed_source_edits_and_key_changes() {
    let a = item(1, &[]);
    let b = item(2, &[1, 2]);
    let c = item(3, &[2]);
    let mut h = Harness::new();
    h.seed(vec![a.clone(), b.clone()]);
    a.set(&[1]);
    b.set(&[3]);
    h.sync();
    h.send(VecDiff::InsertAt {
        index: 1,
        value: c.clone(),
    });
    h.send(VecDiff::Move {
        old_index: 0,
        new_index: 2,
    });
    h.sync();
    h.send(VecDiff::UpdateAt {
        index: 0,
        value: item(4, &[1, 2]),
    });
    h.sync();
    h.send(VecDiff::Push {
        value: item(5, &[]),
    });
    h.send(VecDiff::Pop {});
    h.sync();
    h.send(VecDiff::RemoveAt { index: 1 });
    h.send(VecDiff::Clear {});
    h.sync();
    h.send(VecDiff::Replace {
        values: vec![item(6, &[7]), item(7, &[7, 8])],
    });
    h.sync();
    assert_eq!(&*h.factory.borrow(), &[1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(h.snapshot(), vec![(7, vec![6, 7]), (8, vec![7])]);
}

#[test]
fn mixed_source_and_membership_edits_match_snapshots_and_consumer_identity() {
    let mut h = Harness::new();
    let mut model = Vec::<Item>::new();
    let mut serial = 0;
    let mut state = 721_u32;
    for step in 0..180 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let n = state as usize;
        let operation = n % 9;
        let groups: BTreeMap<_, _> = h.visible.iter().copied().collect();
        let before: BTreeMap<_, _> = h
            .visible
            .iter()
            .flat_map(|(key, _)| {
                h.tokens(*key)
                    .into_iter()
                    .map(move |(id, token)| ((*key, id), token))
            })
            .collect();
        let keys = vec![(n % 4) as i32, ((n / 7) % 4) as i32, (n % 4) as i32];
        let mut diff = None;
        if operation == 0 && !model.is_empty() {
            model[n % model.len()].set(&keys);
        } else if operation == 1 && !model.is_empty() {
            model[n % model.len()].set(&[]);
        } else if operation == 2 && !model.is_empty() {
            diff = Some(VecDiff::Move {
                old_index: n % model.len(),
                new_index: (n / 11) % model.len(),
            });
        } else if operation == 3 && !model.is_empty() {
            serial += 1;
            diff = Some(VecDiff::UpdateAt {
                index: n % model.len(),
                value: item(serial, &keys),
            });
        } else if operation == 4 && !model.is_empty() {
            diff = Some(VecDiff::RemoveAt {
                index: n % model.len(),
            });
        } else if operation == 5 && step % 3 == 0 {
            serial += 2;
            diff = Some(VecDiff::Replace {
                values: vec![item(serial - 1, &keys), item(serial, &[2, 3])],
            });
        } else if operation == 6 && step % 4 == 0 {
            diff = Some(VecDiff::Clear {});
        } else {
            serial += 1;
            diff = Some(VecDiff::InsertAt {
                index: n % (model.len() + 1),
                value: item(serial, &keys),
            });
        }
        if let Some(diff) = diff {
            diff.clone().apply_to_vec(&mut model);
            h.send(diff);
        }
        h.sync();
        let mut expected = BTreeMap::<i32, Vec<i32>>::new();
        for value in &model {
            let mut keys = value.keys.borrow().value.clone();
            keys.sort();
            keys.dedup();
            for key in keys {
                expected.entry(key).or_default().push(value.id);
            }
        }
        assert_eq!(
            h.snapshot(),
            expected.into_iter().collect::<Vec<_>>(),
            "step {}",
            step
        );
        for (key, id) in &h.visible {
            if let Some(old) = groups.get(key) {
                assert_eq!(id, old, "group at step {}", step);
            }
            for (item, token) in h.tokens(*key) {
                if let Some(old) = before.get(&(*key, item)) {
                    assert_eq!(&token, old, "consumer at step {}", step);
                }
            }
        }
    }
}
