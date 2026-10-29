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

use futures_signals::signal_vec::{Always, SignalVec, SignalVecExt, VecDiff};
use futures_util::task::{noop_waker, waker, ArcWake};

#[derive(Clone, Debug)]
struct Item {
    key: i32,
    id: i32,
}

fn item(key: i32, id: i32) -> Item {
    Item { key, id }
}

#[derive(Default)]
struct Feed {
    changes: VecDeque<VecDiff<Item>>,
    ended: bool,
    polls: usize,
    waker: Option<Waker>,
}

struct Input(Rc<RefCell<Feed>>);

impl SignalVec for Input {
    type Item = Item;
    fn poll_vec_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Item>>> {
        let mut feed = self.0.borrow_mut();
        feed.polls += 1;
        if let Some(diff) = feed.changes.pop_front() {
            Poll::Ready(Some(diff))
        } else if feed.ended {
            Poll::Ready(None)
        } else {
            feed.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

// Normal method lookup selects SignalVecExt on Input when the API exists.
// Dereferencing supplies an executable diagnostic on older library revisions.
struct MissingGrouping;
impl MissingGrouping {
    #[allow(dead_code)]
    #[track_caller]
    fn group_by_key<K, F>(&self, _: F) -> Always<(K, Always<Item>)>
    where
        F: FnMut(&Item) -> K,
    {
        panic!("SignalVecExt::group_by_key is unavailable at this revision")
    }
}
impl Deref for Input {
    type Target = MissingGrouping;
    fn deref(&self) -> &Self::Target {
        &MissingGrouping
    }
}

type Child = Pin<Box<dyn SignalVec<Item = (i32, usize)>>>;
type Outer = Pin<Box<dyn SignalVec<Item = (i32, usize)>>>;

struct Observed {
    stream: Option<Child>,
    values: Vec<(i32, usize)>,
    ended: bool,
}

struct Harness {
    feed: Rc<RefCell<Feed>>,
    outer: Option<Outer>,
    groups: Rc<RefCell<BTreeMap<usize, Observed>>>,
    visible: Vec<(i32, usize)>,
    selected: Rc<RefCell<Vec<i32>>>,
    created: Rc<RefCell<Vec<i32>>>,
    outer_ended: bool,
}

impl Harness {
    fn new() -> Self {
        let feed = Rc::new(RefCell::new(Feed::default()));
        let selected = Rc::new(RefCell::new(Vec::new()));
        let created = Rc::new(RefCell::new(Vec::new()));
        let groups = Rc::new(RefCell::new(BTreeMap::new()));
        let seen = selected.clone();
        let records = groups.clone();
        let made = created.clone();
        let next_group = Cell::new(0);
        let next_member = Rc::new(Cell::new(0));
        let outer = Input(feed.clone())
            .group_by_key(move |value: &Item| {
                seen.borrow_mut().push(value.id);
                value.key
            })
            .map(move |(key, stream)| {
                let group_id = next_group.get();
                next_group.set(group_id + 1);
                let made = made.clone();
                let serial = next_member.clone();
                let stream = stream.map(move |value| {
                    made.borrow_mut().push(value.id);
                    let token = serial.get();
                    serial.set(token + 1);
                    (value.id, token)
                });
                records.borrow_mut().insert(
                    group_id,
                    Observed {
                        stream: Some(Box::pin(stream)),
                        values: Vec::new(),
                        ended: false,
                    },
                );
                (key, group_id)
            });
        Self {
            feed,
            outer: Some(Box::pin(outer)),
            groups,
            visible: Vec::new(),
            selected,
            created,
            outer_ended: false,
        }
    }

    fn send(&mut self, diff: VecDiff<Item>) {
        let mut feed = self.feed.borrow_mut();
        assert!(!feed.ended);
        feed.changes.push_back(diff);
        if let Some(waker) = feed.waker.take() {
            waker.wake();
        }
    }

    fn end(&mut self) {
        let mut feed = self.feed.borrow_mut();
        feed.ended = true;
        if let Some(waker) = feed.waker.take() {
            waker.wake();
        }
    }

    fn pump_with(&mut self, wake: &Waker) {
        let mut cx = Context::from_waker(wake);
        loop {
            match self
                .outer
                .as_mut()
                .unwrap()
                .as_mut()
                .poll_vec_change(&mut cx)
            {
                Poll::Ready(Some(diff)) => diff.apply_to_vec(&mut self.visible),
                Poll::Ready(None) => {
                    self.outer_ended = true;
                    break;
                }
                Poll::Pending => break,
            }
        }
    }

    fn pump(&mut self) {
        self.pump_with(&noop_waker());
    }

    fn drain_with(&mut self, id: usize, wake: &Waker) {
        let mut groups = self.groups.borrow_mut();
        let group = groups.get_mut(&id).expect("known group");
        let mut cx = Context::from_waker(wake);
        if group.ended {
            return;
        }
        loop {
            match group
                .stream
                .as_mut()
                .unwrap()
                .as_mut()
                .poll_vec_change(&mut cx)
            {
                Poll::Ready(Some(diff)) => diff.apply_to_vec(&mut group.values),
                Poll::Ready(None) => {
                    group.ended = true;
                    break;
                }
                Poll::Pending => break,
            }
        }
    }

    fn drain(&mut self, id: usize) {
        self.drain_with(id, &noop_waker());
    }

    fn sync(&mut self) {
        self.pump();
        let ids: Vec<_> = self.visible.iter().map(|(_, id)| *id).collect();
        for id in ids {
            self.drain(id);
        }
    }

    fn key_id(&self, key: i32) -> usize {
        self.visible
            .iter()
            .find(|(k, _)| *k == key)
            .expect("group present")
            .1
    }

    fn values(&self, id: usize) -> Vec<i32> {
        self.groups.borrow()[&id]
            .values
            .iter()
            .map(|(v, _)| *v)
            .collect()
    }

    fn tokens(&self, id: usize) -> Vec<(i32, usize)> {
        self.groups.borrow()[&id].values.clone()
    }

    fn snapshot(&self) -> Vec<(i32, Vec<i32>)> {
        self.visible
            .iter()
            .map(|(key, id)| (*key, self.values(*id)))
            .collect()
    }

    fn ended(&self, id: usize) -> bool {
        self.groups.borrow()[&id].ended
    }

    fn seed(&mut self) {
        self.send(VecDiff::Replace {
            values: vec![item(2, 20), item(1, 10), item(2, 21), item(3, 30)],
        });
        self.sync();
    }
}

#[derive(Default)]
struct Awoken(AtomicBool);
impl ArcWake for Awoken {
    fn wake_by_ref(this: &Arc<Self>) {
        this.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn initial_keys_are_sorted_and_coalesced() {
    let mut h = Harness::new();
    h.seed();
    assert_eq!(
        h.snapshot(),
        vec![(1, vec![10]), (2, vec![20, 21]), (3, vec![30])]
    );
}

#[test]
fn new_keys_enter_in_key_order() {
    let mut h = Harness::new();
    h.seed();
    for (key, id) in [(4, 40), (0, 0), (-1, -10)] {
        h.send(VecDiff::Push {
            value: item(key, id),
        });
    }
    h.sync();
    assert_eq!(
        h.visible.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        vec![-1, 0, 1, 2, 3, 4]
    );
}

#[test]
fn insertion_uses_member_rank_at_both_ends() {
    let mut h = Harness::new();
    h.seed();
    h.send(VecDiff::InsertAt {
        index: 0,
        value: item(2, 19),
    });
    h.send(VecDiff::InsertAt {
        index: 5,
        value: item(2, 22),
    });
    h.send(VecDiff::InsertAt {
        index: 3,
        value: item(2, 99),
    });
    h.sync();
    assert_eq!(h.values(h.key_id(2)), vec![19, 20, 99, 21, 22]);
}

#[test]
fn push_into_existing_and_empty_groups() {
    let mut h = Harness::new();
    h.send(VecDiff::Push { value: item(8, 1) });
    h.send(VecDiff::Push { value: item(9, 2) });
    h.send(VecDiff::Push { value: item(8, 3) });
    h.sync();
    assert_eq!(h.snapshot(), vec![(8, vec![1, 3]), (9, vec![2])]);
}

#[test]
fn update_with_same_key_replaces_only_its_position() {
    let mut h = Harness::new();
    h.seed();
    h.send(VecDiff::UpdateAt {
        index: 2,
        value: item(2, 99),
    });
    h.sync();
    assert_eq!(
        h.snapshot(),
        vec![(1, vec![10]), (2, vec![20, 99]), (3, vec![30])]
    );
}

#[test]
fn repeated_updates_of_single_member() {
    let mut h = Harness::new();
    h.send(VecDiff::Push { value: item(0, 1) });
    h.sync();
    let id = h.key_id(0);
    for value in [2, 3, 4] {
        h.send(VecDiff::UpdateAt {
            index: 0,
            value: item(0, value),
        });
    }
    h.sync();
    assert_eq!(h.key_id(0), id);
    assert_eq!(h.values(id), vec![4]);
}

#[test]
fn changed_key_joins_at_source_position() {
    let mut h = Harness::new();
    h.seed();
    h.send(VecDiff::UpdateAt {
        index: 1,
        value: item(2, 99),
    });
    h.sync();
    assert_eq!(h.snapshot(), vec![(2, vec![20, 99, 21]), (3, vec![30])]);
}

#[test]
fn changed_key_creates_a_sorted_group() {
    let mut h = Harness::new();
    h.seed();
    h.send(VecDiff::UpdateAt {
        index: 0,
        value: item(-1, 90),
    });
    h.sync();
    assert_eq!(
        h.snapshot(),
        vec![(-1, vec![90]), (1, vec![10]), (2, vec![21]), (3, vec![30])]
    );
}

#[test]
fn moves_translate_forward_and_backward_indices() {
    let mut h = Harness::new();
    h.seed();
    h.send(VecDiff::Move {
        old_index: 0,
        new_index: 3,
    });
    h.sync();
    assert_eq!(h.values(h.key_id(2)), vec![21, 20]);
    h.send(VecDiff::Move {
        old_index: 3,
        new_index: 0,
    });
    h.sync();
    assert_eq!(h.values(h.key_id(2)), vec![20, 21]);
}

#[test]
fn crossing_other_keys_and_self_moves_preserve_order() {
    let mut h = Harness::new();
    h.seed();
    h.send(VecDiff::Move {
        old_index: 0,
        new_index: 1,
    });
    h.send(VecDiff::Move {
        old_index: 3,
        new_index: 3,
    });
    h.sync();
    assert_eq!(
        h.snapshot(),
        vec![(1, vec![10]), (2, vec![20, 21]), (3, vec![30])]
    );
}

#[test]
fn removal_translates_interleaved_positions() {
    let mut h = Harness::new();
    h.seed();
    h.send(VecDiff::RemoveAt { index: 2 });
    h.sync();
    assert_eq!(
        h.snapshot(),
        vec![(1, vec![10]), (2, vec![20]), (3, vec![30])]
    );
}

#[test]
fn pop_and_remove_first_leave_correct_members() {
    let mut h = Harness::new();
    h.seed();
    h.send(VecDiff::Pop {});
    h.send(VecDiff::RemoveAt { index: 0 });
    h.sync();
    assert_eq!(h.snapshot(), vec![(1, vec![10]), (2, vec![21])]);
}

#[test]
fn retired_group_drains_to_empty_and_ends() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(1);
    h.send(VecDiff::RemoveAt { index: 1 });
    h.pump();
    h.drain(id);
    assert_eq!(h.values(id), Vec::<i32>::new());
    assert!(h.ended(id));
}

#[test]
fn reappearing_key_has_fresh_stream() {
    let mut h = Harness::new();
    h.seed();
    let old = h.key_id(1);
    h.send(VecDiff::RemoveAt { index: 1 });
    h.send(VecDiff::Push { value: item(1, 11) });
    h.sync();
    let new = h.key_id(1);
    assert_ne!(old, new);
    h.drain(old);
    assert!(h.ended(old));
    assert!(h.values(old).is_empty());
    assert_eq!(h.values(new), vec![11]);
}

#[test]
fn replace_preserves_common_group_streams() {
    let mut h = Harness::new();
    h.seed();
    let one = h.key_id(1);
    let two = h.key_id(2);
    h.send(VecDiff::Replace {
        values: vec![item(2, 22), item(1, 11), item(2, 23)],
    });
    h.sync();
    assert_eq!(h.key_id(1), one);
    assert_eq!(h.key_id(2), two);
    assert_eq!(h.values(two), vec![22, 23]);
    assert!(!h.ended(two));
}

#[test]
fn replace_retires_missing_and_introduces_new_keys() {
    let mut h = Harness::new();
    h.seed();
    let old = h.key_id(3);
    h.send(VecDiff::Replace {
        values: vec![item(4, 40), item(2, 22), item(0, 0)],
    });
    h.sync();
    h.drain(old);
    assert_eq!(
        h.snapshot(),
        vec![(0, vec![0]), (2, vec![22]), (4, vec![40])]
    );
    assert!(h.ended(old));
    assert!(h.values(old).is_empty());
}

fn clear_then_reuse(replacement: bool) {
    let mut h = Harness::new();
    h.seed();
    let ids: Vec<_> = h.visible.iter().map(|(_, id)| *id).collect();
    h.send(if replacement {
        VecDiff::Replace { values: vec![] }
    } else {
        VecDiff::Clear {}
    });
    h.sync();
    assert!(h.visible.is_empty());
    for id in ids {
        h.drain(id);
        assert!(h.ended(id));
        assert!(h.values(id).is_empty());
    }
    h.send(VecDiff::Clear {});
    h.send(VecDiff::Push { value: item(2, 90) });
    h.sync();
    assert_eq!(h.snapshot(), vec![(2, vec![90])]);
}
#[test]
fn clear_empties_and_retires_every_group() {
    clear_then_reuse(false);
}
#[test]
fn empty_replace_empties_and_retires_every_group() {
    clear_then_reuse(true);
}

#[test]
fn selector_called_once_for_each_supplied_value() {
    let mut h = Harness::new();
    h.seed();
    h.send(VecDiff::InsertAt {
        index: 0,
        value: item(2, 22),
    });
    h.send(VecDiff::Push { value: item(1, 11) });
    h.send(VecDiff::UpdateAt {
        index: 2,
        value: item(3, 31),
    });
    h.send(VecDiff::Replace {
        values: vec![item(5, 50), item(5, 51)],
    });
    h.sync();
    let mut seen = h.selected.borrow().clone();
    seen.sort();
    assert_eq!(seen, vec![10, 11, 20, 21, 22, 30, 31, 50, 51]);
}

#[test]
fn selector_not_called_for_unchanged_values() {
    let mut h = Harness::new();
    h.seed();
    h.selected.borrow_mut().clear();
    h.send(VecDiff::Move {
        old_index: 0,
        new_index: 3,
    });
    h.send(VecDiff::RemoveAt { index: 0 });
    h.send(VecDiff::Pop {});
    h.send(VecDiff::Clear {});
    h.sync();
    assert!(h.selected.borrow().is_empty());
}

#[test]
fn edits_preserve_unchanged_member_consumers() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    let before = h.tokens(id);
    h.created.borrow_mut().clear();
    h.send(VecDiff::InsertAt {
        index: 1,
        value: item(2, 99),
    });
    h.sync();
    assert_eq!(h.tokens(id)[0], before[0]);
    assert_eq!(h.tokens(id)[2], before[1]);
    h.send(VecDiff::UpdateAt {
        index: 1,
        value: item(2, 98),
    });
    h.sync();
    h.send(VecDiff::RemoveAt { index: 1 });
    h.sync();
    assert_eq!(h.tokens(id), before);
    assert_eq!(*h.created.borrow(), vec![99, 98]);
}

#[test]
fn moves_preserve_mapped_member_identity() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    let mut before = h.tokens(id);
    h.created.borrow_mut().clear();
    h.send(VecDiff::Move {
        old_index: 0,
        new_index: 3,
    });
    h.sync();
    before.reverse();
    assert_eq!(h.tokens(id), before);
    assert!(h.created.borrow().is_empty());
}

#[test]
fn unaffected_groups_keep_stream_identity() {
    let mut h = Harness::new();
    h.seed();
    let original = h.visible.clone();
    h.send(VecDiff::Push { value: item(-1, 0) });
    h.send(VecDiff::InsertAt {
        index: 1,
        value: item(2, 99),
    });
    h.send(VecDiff::UpdateAt {
        index: 1,
        value: item(2, 98),
    });
    h.sync();
    for (key, id) in original {
        assert_eq!(h.key_id(key), id);
        assert!(!h.ended(id));
    }
}

#[test]
fn removing_another_group_preserves_survivors() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    h.send(VecDiff::RemoveAt { index: 1 });
    h.send(VecDiff::Pop {});
    h.sync();
    assert_eq!(h.visible, vec![(2, id)]);
    assert_eq!(h.values(id), vec![20, 21]);
}

#[test]
fn construction_and_child_polling_do_not_drive_source() {
    let mut h = Harness::new();
    assert_eq!(h.feed.borrow().polls, 0);
    h.seed();
    let id = h.key_id(2);
    let polls = h.feed.borrow().polls;
    h.send(VecDiff::Push { value: item(2, 22) });
    h.drain(id);
    assert_eq!(h.feed.borrow().polls, polls);
    assert_eq!(h.values(id), vec![20, 21]);
    h.sync();
    assert_eq!(h.values(id), vec![20, 21, 22]);
}

#[test]
fn inner_only_changes_are_driven_by_outer_polling() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    h.send(VecDiff::Push { value: item(2, 22) });
    h.pump();
    h.drain(id);
    assert_eq!(h.values(id), vec![20, 21, 22]);
    assert!(!h.outer_ended);
}

#[test]
fn slow_member_consumer_receives_every_edit_in_order() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    h.created.borrow_mut().clear();
    h.send(VecDiff::Push { value: item(2, 22) });
    h.pump();
    h.send(VecDiff::UpdateAt {
        index: 4,
        value: item(2, 23),
    });
    h.pump();
    h.send(VecDiff::Pop {});
    h.pump();
    h.drain(id);
    assert_eq!(h.values(id), vec![20, 21]);
    assert_eq!(*h.created.borrow(), vec![22, 23]);
}

#[test]
fn consumers_can_drain_in_different_orders() {
    let mut h = Harness::new();
    h.seed();
    let one = h.key_id(1);
    let two = h.key_id(2);
    h.send(VecDiff::Push { value: item(1, 11) });
    h.send(VecDiff::Push { value: item(2, 22) });
    h.pump();
    h.drain(two);
    assert_eq!(h.values(two), vec![20, 21, 22]);
    assert_eq!(h.values(one), vec![10]);
    h.drain(one);
    assert_eq!(h.values(one), vec![10, 11]);
}

#[test]
fn pending_child_is_woken_for_update() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    let flag = Arc::new(Awoken::default());
    h.drain_with(id, &waker(flag.clone()));
    h.send(VecDiff::Push { value: item(2, 22) });
    h.pump();
    assert!(
        flag.0.load(Ordering::SeqCst),
        "pending child was not notified"
    );
    h.drain(id);
    assert_eq!(h.values(id), vec![20, 21, 22]);
}

#[test]
fn pending_child_is_woken_when_parent_drops() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    let flag = Arc::new(Awoken::default());
    h.drain_with(id, &waker(flag.clone()));
    h.outer.take();
    assert!(
        flag.0.load(Ordering::SeqCst),
        "pending child was not notified of closure"
    );
    h.drain(id);
    assert!(h.ended(id));
}

#[test]
fn parent_drop_drains_queued_changes_without_clearing() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    h.send(VecDiff::Push { value: item(2, 22) });
    h.pump();
    h.outer.take();
    h.drain(id);
    assert_eq!(h.values(id), vec![20, 21, 22]);
    assert!(h.ended(id));
}

#[test]
fn parent_drop_closes_unpolled_new_group() {
    let mut h = Harness::new();
    h.send(VecDiff::Push { value: item(1, 10) });
    h.pump();
    let id = h.key_id(1);
    h.outer.take();
    h.drain(id);
    assert_eq!(h.values(id), vec![10]);
    assert!(h.ended(id));
}

#[test]
fn source_end_drains_all_group_changes() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    h.send(VecDiff::Push { value: item(2, 22) });
    h.end();
    h.pump();
    h.drain(id);
    assert!(h.outer_ended);
    assert_eq!(h.values(id), vec![20, 21, 22]);
    assert!(h.ended(id));
}

#[test]
fn source_end_keeps_new_outer_groups_deliverable() {
    let mut h = Harness::new();
    h.send(VecDiff::Replace {
        values: vec![item(4, 40), item(1, 10)],
    });
    h.end();
    h.sync();
    assert!(h.outer_ended);
    assert_eq!(h.snapshot(), vec![(1, vec![10]), (4, vec![40])]);
    for (_, id) in &h.visible {
        assert!(h.ended(*id));
    }
}

#[test]
fn dropped_child_does_not_block_other_groups() {
    let mut h = Harness::new();
    h.seed();
    let dead = h.key_id(1);
    h.groups.borrow_mut().get_mut(&dead).unwrap().stream.take();
    h.send(VecDiff::Push { value: item(1, 11) });
    h.send(VecDiff::Push { value: item(2, 22) });
    h.pump();
    let alive = h.key_id(2);
    h.drain(alive);
    assert_eq!(h.values(alive), vec![20, 21, 22]);
}

#[test]
fn dropped_child_can_retire_and_its_key_reappear() {
    let mut h = Harness::new();
    h.seed();
    let dead = h.key_id(1);
    h.groups.borrow_mut().get_mut(&dead).unwrap().stream.take();
    h.send(VecDiff::RemoveAt { index: 1 });
    h.send(VecDiff::Push { value: item(1, 11) });
    h.sync();
    assert_ne!(h.key_id(1), dead);
    assert_eq!(h.values(h.key_id(1)), vec![11]);
}

#[test]
fn empty_source_needs_no_executor() {
    let mut h = Harness::new();
    h.send(VecDiff::Replace { values: vec![] });
    h.sync();
    assert!(h.visible.is_empty());
    assert!(h.selected.borrow().is_empty());
    h.send(VecDiff::Push { value: item(7, 70) });
    h.sync();
    assert_eq!(h.snapshot(), vec![(7, vec![70])]);
}

#[test]
fn borrowed_selector_supports_owned_ordered_keys() {
    let feed = Rc::new(RefCell::new(Feed::default()));
    feed.borrow_mut().changes.push_back(VecDiff::Replace {
        values: vec![item(2, 20), item(1, 10)],
    });
    let mut key_buffer = String::new();
    let mut stream = Box::pin(
        Input(feed)
            .group_by_key(move |value: &Item| {
                key_buffer.clear();
                key_buffer.push_str(&value.key.to_string());
                key_buffer.clone()
            })
            .map(|(key, _)| key),
    );
    let mut keys = vec![];
    let wake = noop_waker();
    let mut cx = Context::from_waker(&wake);
    while let Poll::Ready(Some(diff)) = stream.as_mut().poll_vec_change(&mut cx) {
        diff.apply_to_vec(&mut keys);
    }
    assert_eq!(keys, vec!["1", "2"]);
}

#[test]
fn mixed_edits_match_snapshots_identities_and_selector_counts() {
    let mut h = Harness::new();
    let mut source: Vec<Item> = vec![];
    let mut serial = 100;
    let mut random = 17u32;
    for step in 0..160 {
        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
        serial += 1;
        let value = item((random % 5) as i32 - 2, serial);
        let position = random as usize % (source.len() + 1);
        let diff = match step % 10 {
            0 => VecDiff::Replace {
                values: (0..(step % 7 + 1))
                    .map(|n| item((n % 3) as i32, serial * 100 + n as i32))
                    .collect(),
            },
            1 | 6 => VecDiff::InsertAt {
                index: position,
                value,
            },
            2 | 7 if !source.is_empty() => VecDiff::UpdateAt {
                index: position % source.len(),
                value,
            },
            3 if !source.is_empty() => VecDiff::Move {
                old_index: position % source.len(),
                new_index: source.len() - 1,
            },
            4 if !source.is_empty() => VecDiff::Move {
                old_index: source.len() - 1,
                new_index: 0,
            },
            5 if !source.is_empty() => VecDiff::RemoveAt {
                index: position % source.len(),
            },
            8 if !source.is_empty() => VecDiff::Pop {},
            9 => VecDiff::Clear {},
            _ => VecDiff::Push { value },
        };
        let is_replace = matches!(&diff, VecDiff::Replace { .. });
        let old_groups: BTreeMap<_, _> = h.visible.iter().cloned().collect();
        let old_members: BTreeMap<_, _> =
            h.visible.iter().flat_map(|(_, id)| h.tokens(*id)).collect();
        let mut supplied = match &diff {
            VecDiff::Replace { values } => values.iter().map(|v| v.id).collect::<Vec<_>>(),
            VecDiff::InsertAt { value, .. }
            | VecDiff::UpdateAt { value, .. }
            | VecDiff::Push { value } => vec![value.id],
            _ => vec![],
        };
        diff.clone().apply_to_vec(&mut source);
        let mut expected = BTreeMap::<i32, Vec<i32>>::new();
        for value in &source {
            expected.entry(value.key).or_default().push(value.id);
        }
        h.selected.borrow_mut().clear();
        h.send(diff);
        h.sync();
        assert_eq!(
            h.snapshot(),
            expected.clone().into_iter().collect::<Vec<_>>(),
            "snapshot at edit {}",
            step
        );
        let mut selected = h.selected.borrow().clone();
        selected.sort();
        supplied.sort();
        assert_eq!(selected, supplied, "selector accounting at edit {}", step);
        for (&key, &old_id) in &old_groups {
            if expected.contains_key(&key) {
                assert_eq!(h.key_id(key), old_id, "group identity at edit {}", step);
            } else {
                h.drain(old_id);
                assert!(h.ended(old_id));
                assert!(h.values(old_id).is_empty());
            }
        }
        if !is_replace {
            for (_, id) in &h.visible {
                for (value, token) in h.tokens(*id) {
                    if let Some(previous) = old_members.get(&value) {
                        assert_eq!(token, *previous, "member identity at edit {}", step);
                    }
                }
            }
        }
    }
}

#[test]
fn replacement_changes_cardinality_before_later_removals() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    h.send(VecDiff::Replace {
        values: vec![item(2, 90), item(2, 91), item(2, 92), item(2, 93)],
    });
    h.sync();
    for remaining in (1..4).rev() {
        h.send(VecDiff::Pop {});
        h.sync();
        assert_eq!(h.visible, vec![(2, id)]);
        assert_eq!(h.values(id).len(), remaining);
    }
    h.send(VecDiff::Pop {});
    h.sync();
    h.drain(id);
    assert!(h.visible.is_empty());
    assert!(h.values(id).is_empty());
    assert!(h.ended(id));
}

#[test]
fn outer_consumer_is_notified_by_source_changes() {
    let mut h = Harness::new();
    h.seed();
    let flag = Arc::new(Awoken::default());
    let wake = waker(flag.clone());
    h.pump_with(&wake);
    h.send(VecDiff::Push { value: item(2, 22) });
    assert!(
        flag.0.swap(false, Ordering::SeqCst),
        "source readiness did not notify the outer consumer"
    );
    h.pump_with(&wake);
    flag.0.store(false, Ordering::SeqCst);
    h.end();
    assert!(
        flag.0.load(Ordering::SeqCst),
        "source completion did not notify the outer consumer"
    );
}

#[test]
fn lagging_consumer_replays_a_burst_without_loss() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    h.created.borrow_mut().clear();
    for value in 100..164 {
        h.send(VecDiff::Push {
            value: item(2, value),
        });
    }
    h.end();
    let flag = Arc::new(Awoken::default());
    let wake = waker(flag.clone());
    flag.0.store(true, Ordering::SeqCst);
    while flag.0.swap(false, Ordering::SeqCst) {
        if !h.outer_ended {
            h.pump_with(&wake);
        }
        let ids: Vec<_> = h.groups.borrow().keys().cloned().collect();
        for id in ids {
            h.drain_with(id, &wake);
        }
    }
    let mut expected = vec![20, 21];
    expected.extend(100..164);
    assert_eq!(h.values(id), expected);
    assert_eq!(*h.created.borrow(), (100..164).collect::<Vec<_>>());
    assert!(h.outer_ended);
    assert!(h.ended(id));
}

#[test]
fn dropped_consumer_keeps_membership_and_common_key_identity() {
    let mut h = Harness::new();
    h.seed();
    let dead = h.key_id(1);
    let live = h.key_id(2);
    h.groups.borrow_mut().get_mut(&dead).unwrap().stream.take();
    h.send(VecDiff::Push { value: item(1, 11) });
    h.send(VecDiff::RemoveAt { index: 1 });
    h.pump();
    assert_eq!(
        h.key_id(1),
        dead,
        "an unobserved group still has a source member"
    );
    h.send(VecDiff::Replace {
        values: vec![item(2, 22), item(1, 12), item(2, 23)],
    });
    h.pump();
    assert_eq!(h.visible, vec![(1, dead), (2, live)]);
    h.drain(live);
    assert_eq!(h.values(live), vec![22, 23]);
}

#[test]
fn updates_in_a_dropped_group_can_migrate_to_live_groups() {
    let mut h = Harness::new();
    h.seed();
    let dead = h.key_id(1);
    let live = h.key_id(2);
    h.groups.borrow_mut().get_mut(&dead).unwrap().stream.take();
    h.selected.borrow_mut().clear();
    h.send(VecDiff::UpdateAt {
        index: 1,
        value: item(1, 11),
    });
    h.pump();
    h.send(VecDiff::UpdateAt {
        index: 1,
        value: item(2, 99),
    });
    h.pump();
    assert_eq!(*h.selected.borrow(), vec![11, 99]);
    assert_eq!(
        h.visible.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(h.key_id(2), live);
    h.drain(live);
    assert_eq!(h.values(live), vec![20, 99, 21]);
}

#[test]
fn dropping_outer_does_not_poll_unconsumed_source_edits() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    let before = h.feed.borrow().polls;
    h.selected.borrow_mut().clear();
    h.send(VecDiff::Push { value: item(2, 22) });
    h.outer.take();
    h.drain(id);
    assert_eq!(h.feed.borrow().polls, before);
    assert!(h.selected.borrow().is_empty());
    assert_eq!(h.values(id), vec![20, 21]);
    assert!(h.ended(id));
}

#[test]
fn public_api_accepts_only_the_documented_bounds() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let targets = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let result = std::process::Command::new("cargo")
        .arg("check")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(root.join("tests/fixtures/keyed_api/Cargo.toml"))
        .arg("--target-dir")
        .arg(targets.join("api-admission"))
        .output()
        .expect("run the installed Rust build tool");
    assert!(
        result.status.success(),
        "valid public API inputs were rejected:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

fn drive_notified_with_history(h: &mut Harness) -> BTreeMap<usize, Vec<Vec<i32>>> {
    let flag = Arc::new(Awoken::default());
    let wake = waker(flag.clone());
    let mut cx = Context::from_waker(&wake);
    let mut history = BTreeMap::<usize, Vec<Vec<i32>>>::new();
    flag.0.store(true, Ordering::SeqCst);
    while flag.0.swap(false, Ordering::SeqCst) {
        if !h.outer_ended {
            h.pump_with(&wake);
        }
        let mut groups = h.groups.borrow_mut();
        for (id, group) in groups.iter_mut() {
            if group.ended || group.stream.is_none() {
                continue;
            }
            loop {
                match group
                    .stream
                    .as_mut()
                    .unwrap()
                    .as_mut()
                    .poll_vec_change(&mut cx)
                {
                    Poll::Ready(Some(diff)) => {
                        diff.apply_to_vec(&mut group.values);
                        history
                            .entry(*id)
                            .or_default()
                            .push(group.values.iter().map(|(value, _)| *value).collect());
                    }
                    Poll::Ready(None) => {
                        group.ended = true;
                        break;
                    }
                    Poll::Pending => break,
                }
            }
        }
    }
    history
}

#[test]
fn unpolled_child_replays_edits_before_retirement() {
    let mut h = Harness::new();
    h.send(VecDiff::Push { value: item(1, 10) });
    h.pump();
    let old = h.key_id(1);
    h.send(VecDiff::UpdateAt {
        index: 0,
        value: item(1, 11),
    });
    h.send(VecDiff::Push { value: item(1, 12) });
    h.send(VecDiff::Clear {});
    h.send(VecDiff::Push { value: item(1, 13) });
    drive_notified_with_history(&mut h);
    let new = h.key_id(1);
    assert_ne!(old, new);
    assert_eq!(
        h.created
            .borrow()
            .iter()
            .copied()
            .filter(|value| *value != 13)
            .collect::<Vec<_>>(),
        vec![10, 11, 12]
    );
    assert!(h.values(old).is_empty());
    assert!(h.ended(old));
    assert_eq!(h.values(new), vec![13]);
}

#[test]
fn lagging_child_preserves_each_edit_state() {
    let mut h = Harness::new();
    h.seed();
    let id = h.key_id(2);
    h.send(VecDiff::Move {
        old_index: 0,
        new_index: 3,
    });
    h.send(VecDiff::Move {
        old_index: 3,
        new_index: 0,
    });
    h.send(VecDiff::Push { value: item(2, 88) });
    h.send(VecDiff::Pop {});
    h.send(VecDiff::RemoveAt { index: 0 });
    h.send(VecDiff::InsertAt {
        index: 0,
        value: item(2, 22),
    });
    h.send(VecDiff::UpdateAt {
        index: 0,
        value: item(2, 23),
    });
    h.send(VecDiff::UpdateAt {
        index: 2,
        value: item(2, 26),
    });
    h.send(VecDiff::UpdateAt {
        index: 0,
        value: item(2, 24),
    });
    h.send(VecDiff::UpdateAt {
        index: 0,
        value: item(2, 25),
    });
    h.send(VecDiff::Replace {
        values: vec![item(2, 90), item(1, 10)],
    });
    h.send(VecDiff::Replace {
        values: vec![item(2, 91), item(2, 92), item(1, 11)],
    });
    h.send(VecDiff::RemoveAt { index: 0 });
    h.send(VecDiff::RemoveAt { index: 0 });
    let history = drive_notified_with_history(&mut h);
    assert_eq!(
        h.visible.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
        vec![1]
    );
    assert!(h.ended(id));
    let expected = vec![
        vec![21, 20],
        vec![20, 21],
        vec![20, 21, 88],
        vec![20, 21],
        vec![21],
        vec![22, 21],
        vec![23, 21],
        vec![23, 26],
        vec![24, 26],
        vec![25, 26],
        vec![90],
        vec![91, 92],
        vec![92],
        vec![],
    ];
    let actual = history.get(&id).cloned().unwrap_or_default();
    let mut observed = actual.iter();
    for state in &expected {
        assert!(
            observed.by_ref().any(|value| value == state),
            "missing ordered member state {:?}; observed {:?}",
            state,
            actual
        );
    }
}

#[test]
fn insertion_before_dropped_group_preserves_later_update_target() {
    let mut h = Harness::new();
    h.seed();
    let dropped = h.key_id(1);
    let live = h.key_id(2);
    h.groups
        .borrow_mut()
        .get_mut(&dropped)
        .unwrap()
        .stream
        .take();
    h.send(VecDiff::InsertAt {
        index: 0,
        value: item(2, 99),
    });
    h.send(VecDiff::UpdateAt {
        index: 2,
        value: item(2, 11),
    });
    drive_notified_with_history(&mut h);
    assert_eq!(
        h.visible.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(h.key_id(2), live);
    assert_eq!(h.values(live), vec![99, 20, 11, 21]);
}
