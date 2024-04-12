use std::task::Poll;
use futures_signals::signal_vec::{MutableVec, SignalVecExt, VecDiff, from_stream};

mod util;


#[test]
fn sync() {
    let _: Box<dyn Send + Sync> = Box::new(MutableVec::<()>::new());
    let _: Box<dyn Send + Sync> = Box::new(MutableVec::<()>::new().signal_vec());
    let _: Box<dyn Send + Sync> = Box::new(MutableVec::<()>::new().signal_vec_cloned());

    let _: Box<dyn Send + Sync> = Box::new(MutableVec::<()>::new_with_values(vec![]));
    let _: Box<dyn Send + Sync> = Box::new(MutableVec::<()>::new_with_values(vec![]).signal_vec());
    let _: Box<dyn Send + Sync> = Box::new(MutableVec::<()>::new_with_values(vec![]).signal_vec_cloned());
}


#[test]
fn chain_replace() {
    {
        let left = util::Source::new(vec![] as Vec<Poll<VecDiff<u32>>>);

        let right = util::Source::new(vec![] as Vec<Poll<VecDiff<u32>>>);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Replace { values: vec![6, 7] }),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Push { value: 11 }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(Some(VecDiff::RemoveAt { index: 2 })),
            Poll::Ready(Some(VecDiff::RemoveAt { index: 1 })),
            Poll::Ready(Some(VecDiff::RemoveAt { index: 0 })),
            Poll::Ready(Some(VecDiff::InsertAt { index: 0, value: 6 })),
            Poll::Ready(Some(VecDiff::InsertAt { index: 1, value: 7 })),
            Poll::Ready(Some(VecDiff::Push { value: 11 })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Replace { values: vec![] }),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Replace { values: vec![6, 7, 8] }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(Some(VecDiff::Replace { values: vec![6, 7, 8] })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
        ]);

        let right = util::Source::new(vec![
            Poll::Pending,
            Poll::Ready(VecDiff::Push { value: 11 }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2] })),
            Poll::Ready(Some(VecDiff::Replace { values: vec![3, 4, 5] })),
            Poll::Ready(Some(VecDiff::Push { value: 11 })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Push { value: 10 }),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Replace { values: vec![6, 7] }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(Some(VecDiff::InsertAt { index: 3, value: 10 })),
            Poll::Ready(Some(VecDiff::Pop {})),
            Poll::Ready(Some(VecDiff::Pop {})),
            Poll::Ready(Some(VecDiff::Pop {})),
            Poll::Ready(Some(VecDiff::Push { value: 6 })),
            Poll::Ready(Some(VecDiff::Push { value: 7 })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Push { value: 10 }),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Push { value: 11 }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(Some(VecDiff::InsertAt { index: 3, value: 10 })),
            Poll::Ready(Some(VecDiff::Push { value: 11 })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Push { value: 10 }),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Push { value: 11 }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2] })),
            Poll::Ready(Some(VecDiff::Push { value: 11 })),
            Poll::Ready(Some(VecDiff::InsertAt { index: 3, value: 10 })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![3, 4, 5] })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Push { value: 10 }),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Push { value: 11 }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Push { value: 10 })),
            Poll::Ready(Some(VecDiff::Push { value: 3 })),
            Poll::Ready(Some(VecDiff::Push { value: 4 })),
            Poll::Ready(Some(VecDiff::Push { value: 5 })),
            Poll::Ready(Some(VecDiff::Push { value: 11 })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Pending,
            Poll::Pending,
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
        ]);

        let right = util::Source::new(vec![
            Poll::Pending,
            Poll::Pending,
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Pending,
            Poll::Pending,
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(None),
        ]);
    }
}

#[test]
fn chain_push() {
    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Push { value: 10 }),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Push { value: 11 }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(Some(VecDiff::InsertAt { index: 3, value: 10 })),
            Poll::Ready(Some(VecDiff::Push { value: 11 })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Push { value: 10 }),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Push { value: 11 }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(Some(VecDiff::InsertAt { index: 3, value: 10 })),
            Poll::Ready(Some(VecDiff::Push { value: 11 })),
            Poll::Ready(None),
        ]);
    }
}

#[test]
fn chain_clear() {
    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Clear {}),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Clear {}),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(Some(VecDiff::Clear {})),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Clear {}),
        ]);

        let right = util::Source::new(vec![
            Poll::Pending,
            Poll::Ready(VecDiff::Push { value: 11 }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2] })),
            Poll::Ready(Some(VecDiff::Clear {})),
            Poll::Ready(Some(VecDiff::Push { value: 11 })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Clear {}),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Push { value: 11 }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(Some(VecDiff::RemoveAt { index: 2 })),
            Poll::Ready(Some(VecDiff::RemoveAt { index: 1 })),
            Poll::Ready(Some(VecDiff::RemoveAt { index: 0 })),
            Poll::Ready(Some(VecDiff::Push { value: 11 })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Push { value: 10 }),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Clear {}),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(Some(VecDiff::InsertAt { index: 3, value: 10 })),
            Poll::Ready(Some(VecDiff::Pop {})),
            Poll::Ready(Some(VecDiff::Pop {})),
            Poll::Ready(Some(VecDiff::Pop {})),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Clear {}),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Push { value: 11 }),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(Some(VecDiff::RemoveAt { index: 2 })),
            Poll::Ready(Some(VecDiff::RemoveAt { index: 1 })),
            Poll::Ready(Some(VecDiff::RemoveAt { index: 0 })),
            Poll::Ready(Some(VecDiff::Push { value: 11 })),
            Poll::Ready(None),
        ]);
    }

    {
        let left = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2] }),
            Poll::Ready(VecDiff::Push { value: 10 }),
        ]);

        let right = util::Source::new(vec![
            Poll::Ready(VecDiff::Replace { values: vec![3, 4, 5] }),
            Poll::Ready(VecDiff::Clear {}),
        ]);

        util::assert_signal_vec_eq(left.chain(right), vec![
            Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
            Poll::Ready(Some(VecDiff::InsertAt { index: 3, value: 10 })),
            Poll::Ready(Some(VecDiff::Pop {})),
            Poll::Ready(Some(VecDiff::Pop {})),
            Poll::Ready(Some(VecDiff::Pop {})),
            Poll::Ready(None),
        ]);
    }
}


#[test]
fn filter() {
    /*#[derive(Debug, PartialEq, Eq)]
    struct Change {
        length: usize,
        indexes: Vec<bool>,
        change: VecDiff<u32>,
    }*/

    let input = util::Source::new(vec![
        Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 0, value: 6 }),
        Poll::Ready(VecDiff::InsertAt { index: 2, value: 7 }),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 5, value: 8 }),
        Poll::Ready(VecDiff::InsertAt { index: 7, value: 9 }),
        Poll::Ready(VecDiff::InsertAt { index: 9, value: 10 }),
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 11, value: 11 }),
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 0, value: 0 }),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 1, value: 0 }),
        Poll::Ready(VecDiff::InsertAt { index: 5, value: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 5, value: 12 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Ready(VecDiff::RemoveAt { index: 1 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
    ]);

    let output = input.filter(|&x| x == 3 || x == 4 || x > 5);

    //assert_eq!(Filter::len(&output), 0);
    //assert_eq!(output.indexes, vec![]);

    let changes = util::map_poll_vec(output, |_output, change| {
        change
        /*Change {
            change: change,
            length: Filter::len(&output),
            indexes: output.indexes.clone(),
        }*/
    });

    assert_eq!(changes, vec![
        Poll::Ready(Some(VecDiff::Replace { values: vec![3, 4] })),
        Poll::Pending,
        Poll::Ready(Some(VecDiff::InsertAt { index: 0, value: 6 })),
        Poll::Ready(Some(VecDiff::InsertAt { index: 1, value: 7 })),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(VecDiff::InsertAt { index: 2, value: 8 })),
        Poll::Ready(Some(VecDiff::InsertAt { index: 4, value: 9 })),
        Poll::Ready(Some(VecDiff::InsertAt { index: 6, value: 10 })),
        Poll::Pending,
        Poll::Ready(Some(VecDiff::InsertAt { index: 7, value: 11 })),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(VecDiff::InsertAt { index: 2, value: 12 })),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(VecDiff::RemoveAt { index: 0 })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 0 })),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(VecDiff::RemoveAt { index: 0 })),
        Poll::Ready(None),

        /*Change { length: 2, indexes: vec![false, false, false, true, true, false], change: VecDiff::Replace { values: vec![3, 4] } },
        Change { length: 3, indexes: vec![true, false, false, false, true, true, false], change: VecDiff::InsertAt { index: 0, value: 6 } },
        Change { length: 4, indexes: vec![true, false, true, false, false, true, true, false], change: VecDiff::InsertAt { index: 1, value: 7 } },
        Change { length: 5, indexes: vec![true, false, true, false, false, true, true, true, false], change: VecDiff::InsertAt { index: 2, value: 8 } },
        Change { length: 6, indexes: vec![true, false, true, false, false, true, true, true, true, false], change: VecDiff::InsertAt { index: 4, value: 9 } },
        Change { length: 7, indexes: vec![true, false, true, false, false, true, true, true, true, true, false], change: VecDiff::InsertAt { index: 6, value: 10 } },
        Change { length: 8, indexes: vec![true, false, true, false, false, true, true, true, true, true, false, true], change: VecDiff::InsertAt { index: 7, value: 11 } },
        Change { length: 9, indexes: vec![false, false, true, false, true, true, false, false, false, true, true, true, true, true, false, true], change: VecDiff::InsertAt { index: 2, value: 12 } },
        Change { length: 8, indexes: vec![false, true, true, false, false, false, true, true, true, true, true, false, true], change: VecDiff::RemoveAt { index: 0 } },
        Change { length: 7, indexes: vec![false, true, false, false, false, true, true, true, true, true, false, true], change: VecDiff::RemoveAt { index: 0 } },
        Change { length: 6, indexes: vec![false, false, false, true, true, true, true, true, false, true], change: VecDiff::RemoveAt { index: 0 } },*/
    ]);
}


#[test]
fn filter_map() {
    let input = util::Source::new(vec![
        Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 0, value: 6 }),
        Poll::Ready(VecDiff::InsertAt { index: 2, value: 7 }),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 5, value: 8 }),
        Poll::Ready(VecDiff::InsertAt { index: 7, value: 9 }),
        Poll::Ready(VecDiff::InsertAt { index: 9, value: 10 }),
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 11, value: 11 }),
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 0, value: 0 }),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 1, value: 0 }),
        Poll::Ready(VecDiff::InsertAt { index: 5, value: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 5, value: 12 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Ready(VecDiff::RemoveAt { index: 1 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
    ]);

    let output = input.filter_map(|x| {
        if x == 3 || x == 4 || x > 5 {
            Some(x + 200)

        } else {
            None
        }
    });

    let changes = util::map_poll_vec(output, |_output, change| change);

    assert_eq!(changes, vec![
        Poll::Ready(Some(VecDiff::Replace { values: vec![203, 204] })),
        Poll::Pending,
        Poll::Ready(Some(VecDiff::InsertAt { index: 0, value: 206 })),
        Poll::Ready(Some(VecDiff::InsertAt { index: 1, value: 207 })),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(VecDiff::InsertAt { index: 2, value: 208 })),
        Poll::Ready(Some(VecDiff::InsertAt { index: 4, value: 209 })),
        Poll::Ready(Some(VecDiff::InsertAt { index: 6, value: 210 })),
        Poll::Pending,
        Poll::Ready(Some(VecDiff::InsertAt { index: 7, value: 211 })),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(VecDiff::InsertAt { index: 2, value: 212 })),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(VecDiff::RemoveAt { index: 0 })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 0 })),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(VecDiff::RemoveAt { index: 0 })),
        Poll::Ready(None),
    ]);
}


#[test]
fn sum() {
    let input = util::Source::new(vec![
        Poll::Pending,
        Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 0, value: 6 }),
        Poll::Ready(VecDiff::InsertAt { index: 2, value: 7 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Ready(VecDiff::UpdateAt { index: 4, value: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Move { old_index: 1, new_index: 3 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 1 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Clear {}),
    ]);

    let output = input.sum();

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(0)),
        Poll::Ready(Some(15)),
        Poll::Pending,
        Poll::Ready(Some(28)),
        Poll::Ready(Some(19)),
        Poll::Pending,
        Poll::Ready(Some(18)),
        Poll::Ready(Some(0)),
        Poll::Ready(None),
    ]);
}


#[test]
fn len() {
    let input = util::Source::new(vec![
        Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 0, value: 6 }),
        Poll::Ready(VecDiff::InsertAt { index: 2, value: 7 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Ready(VecDiff::UpdateAt { index: 4, value: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Move { old_index: 1, new_index: 3 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 1 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Clear {}),
        Poll::Ready(VecDiff::Replace { values: vec![] }),
    ]);

    let output = input.len();

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(6)),
        Poll::Pending,
        Poll::Ready(Some(8)),
        Poll::Ready(Some(7)),
        Poll::Pending,
        Poll::Ready(Some(6)),
        Poll::Ready(Some(0)),
        Poll::Ready(None),
    ]);
}


#[test]
fn is_empty() {
    let input = util::Source::new(vec![
        Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 0, value: 6 }),
        Poll::Ready(VecDiff::InsertAt { index: 2, value: 7 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Ready(VecDiff::UpdateAt { index: 4, value: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Move { old_index: 1, new_index: 3 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 1 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Clear {}),
        Poll::Ready(VecDiff::Replace { values: vec![] }),
    ]);

    let output = input.is_empty();

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(true)),
        Poll::Ready(None),
    ]);
}


#[test]
fn to_signal_map() {
    let input = util::Source::new(vec![
        Poll::Pending,
        Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 0, value: 6 }),
        Poll::Ready(VecDiff::InsertAt { index: 2, value: 7 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Ready(VecDiff::UpdateAt { index: 4, value: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Move { old_index: 1, new_index: 3 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 1 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Clear {}),
    ]);

    let output = input.to_signal_map(|x| x.into_iter().copied().collect::<Vec<u32>>());

    // TODO include the Pending in the output
    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(vec![])),
        Poll::Ready(Some(vec![0, 1, 2, 3, 4, 5])),
        Poll::Pending,
        Poll::Ready(Some(vec![6, 0, 7, 1, 2, 3, 4, 5])),
        Poll::Ready(Some(vec![0, 7, 1, 2, 0, 4, 5])),
        Poll::Ready(Some(vec![0, 1, 2, 7, 0, 4, 5])),
        Poll::Ready(Some(vec![0, 2, 7, 0, 4, 5])),
        Poll::Ready(Some(vec![])),
        Poll::Ready(None),
    ]);
}


#[test]
fn to_signal_cloned() {
    let input = util::Source::new(vec![
        Poll::Pending,
        Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 0, value: 6 }),
        Poll::Ready(VecDiff::InsertAt { index: 2, value: 7 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Ready(VecDiff::UpdateAt { index: 4, value: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Move { old_index: 1, new_index: 3 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 1 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Clear {}),
    ]);

    let output = input.to_signal_cloned();

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(vec![])),
        Poll::Ready(Some(vec![0, 1, 2, 3, 4, 5])),
        Poll::Pending,
        Poll::Ready(Some(vec![6, 0, 7, 1, 2, 3, 4, 5])),
        Poll::Ready(Some(vec![0, 7, 1, 2, 0, 4, 5])),
        Poll::Ready(Some(vec![0, 1, 2, 7, 0, 4, 5])),
        Poll::Ready(Some(vec![0, 2, 7, 0, 4, 5])),
        Poll::Ready(Some(vec![])),
        Poll::Ready(None),
    ]);
}

#[test]
fn debug_to_signal_cloned() {
    let input: util::Source<VecDiff<u32>> = util::Source::new(vec![]);
    assert_eq!(format!("{:?}", input.to_signal_cloned()), "ToSignalCloned { ... }");
}


#[test]
fn test_from_stream() {
    let input = futures_util::stream::iter(vec![1, 2, 3, 4, 5]);

    let output = from_stream(input);

    let changes = util::map_poll_vec(output, |_output, change| change);

    assert_eq!(changes, vec![
        Poll::Ready(Some(VecDiff::Push { value: 1 })),
        Poll::Ready(Some(VecDiff::Push { value: 2 })),
        Poll::Ready(Some(VecDiff::Push { value: 3 })),
        Poll::Ready(Some(VecDiff::Push { value: 4 })),
        Poll::Ready(Some(VecDiff::Push { value: 5 })),
        Poll::Ready(None),
    ]);
}


#[test]
fn flatten() {
    let input = util::Source::new(vec![
        Poll::Ready(VecDiff::Replace {
            values: vec![
                util::Source::new(vec![
                    Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
                    Poll::Pending,
                    Poll::Ready(VecDiff::Replace { values: vec![2, 3, 4] }),
                ]),
                util::Source::new(vec![
                    Poll::Ready(VecDiff::Replace { values: vec![6, 7, 8, 9] }),
                    Poll::Pending,
                    Poll::Ready(VecDiff::Clear {}),
                ]),
            ],
        }),
    ]);

    let output = input.flatten();

    util::assert_signal_vec_eq(output, vec![
        Poll::Ready(Some(VecDiff::Replace {
            values: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 5 })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 4 })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 3 })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 2 })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 1 })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 0 })),
        Poll::Ready(Some(VecDiff::InsertAt { index: 0, value: 2 })),
        Poll::Ready(Some(VecDiff::InsertAt { index: 1, value: 3 })),
        Poll::Ready(Some(VecDiff::InsertAt { index: 2, value: 4 })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 6 })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 5 })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 4 })),
        Poll::Ready(Some(VecDiff::RemoveAt { index: 3 })),
        Poll::Ready(None),
    ]);
}


#[test]
fn flatten_empty() {
    let input = util::Source::new(vec![
        Poll::Pending,
        Poll::Ready(VecDiff::Push {
            value: util::Source::new(vec![]),
        }),
        Poll::Ready(VecDiff::Push {
            value: util::Source::new(vec![
                Poll::Ready(VecDiff::Replace { values: vec![42] }),
            ]),
        }),
    ]);

    let output = input.flatten();

    util::assert_signal_vec_eq(output, vec![
        Poll::Pending,
        Poll::Ready(Some(VecDiff::InsertAt { index: 0, value: 42 })),
        Poll::Ready(None),
    ]);
}
