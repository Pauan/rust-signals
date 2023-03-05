use std::collections::BTreeMap;
use std::task::Poll;
use futures_signals::signal::{self, SignalExt};
use futures_signals::signal_map::{self, MapDiff, SignalMapExt};

mod util;

#[test]
fn map_value() {
    let input = util::Source::new(vec![
        Poll::Ready(MapDiff::Replace {
            entries: vec![(1, 1), (2, 1), (3, 2), (4, 3)]
        }),
        Poll::Pending,
        Poll::Ready(MapDiff::Insert {
            key: 5,
            value: 5,
        }),
        Poll::Ready(MapDiff::Update {
            key: 1,
            value: 0,
        }),
        Poll::Pending,
        Poll::Ready(MapDiff::Remove {key: 1}),
        Poll::Ready(MapDiff::Insert {
            key: 1,
            value: 1,
        }),
        Poll::Ready(MapDiff::Clear {})
    ]);

    let output = input.map_value(|value| value * 2);

    util::assert_signal_map_eq(output, vec![
        Poll::Ready(Some(MapDiff::Replace {
            entries: vec![(1, 2), (2, 2), (3, 4), (4, 6)]
        })),
        Poll::Pending,
        Poll::Ready(Some(MapDiff::Insert {
            key: 5,
            value: 10,
        })),
        Poll::Ready(Some(MapDiff::Update {
            key: 1,
            value: 0,
        })),
        Poll::Pending,
        Poll::Ready(Some(MapDiff::Remove {key: 1})),
        Poll::Ready(Some(MapDiff::Insert {
            key: 1,
            value: 2,
        })),
        Poll::Ready(Some(MapDiff::Clear {})),
        Poll::Ready(None),
    ]);
}

#[test]
fn map_value_signal_const_signal() {
    let input = util::Source::new(vec![
        Poll::Ready(MapDiff::Replace {
            entries: vec![(1, 1), (2, 1), (3, 2), (4, 3)]
        }),
        Poll::Pending,
        Poll::Ready(MapDiff::Insert {
            key: 5,
            value: 5,
        }),
        Poll::Ready(MapDiff::Update {
            key: 1,
            value: 0,
        }),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(MapDiff::Remove {key: 1}),
        Poll::Ready(MapDiff::Insert {
            key: 1,
            value: 1,
        }),
        Poll::Ready(MapDiff::Clear {})
    ]);

    let output = input.map_value_signal(|value| {
        signal::always(2).map(move |multiplier| value * multiplier)
    });

    util::assert_signal_map_eq(output, vec![
        Poll::Ready(Some(MapDiff::Replace {
            entries: vec![(1, 2), (2, 2), (3, 4), (4, 6)]
        })),
        Poll::Ready(Some(MapDiff::Insert {
            key: 5,
            value: 10,
        })),
        Poll::Ready(Some(MapDiff::Update {
            key: 1,
            value: 0,
        })),
        Poll::Pending,
        Poll::Ready(Some(MapDiff::Remove {key: 1})),
        Poll::Ready(Some(MapDiff::Insert {
            key: 1,
            value: 2,
        })),
        Poll::Ready(Some(MapDiff::Clear {})),
        Poll::Ready(None),
    ]);
}

#[test]
fn map_value_signal_const_map() {
    let map_signal = signal_map::always(
        BTreeMap::from([(1, 1), (2, 1), (3, 2), (4, 3)])
    );

    let output = map_signal.map_value_signal(|value| {
        let input = util::Source::new(vec![
            Poll::Ready(2),
            Poll::Pending,
            Poll::Ready(3),
        ]);
        input.map(move |multiplier| value * multiplier)
    });

    util::assert_signal_map_eq(output, vec![
        Poll::Ready(Some(MapDiff::Replace {
            entries: vec![(1, 2), (2, 2), (3, 4), (4, 6)],
        })),
        Poll::Ready(Some(MapDiff::Update { key: 1, value: 3 })),
        Poll::Ready(Some(MapDiff::Update { key: 2, value: 3 })),
        Poll::Ready(Some(MapDiff::Update { key: 3, value: 6 })),
        Poll::Ready(Some(MapDiff::Update { key: 4, value: 9 })),
        Poll::Ready(None),
    ]);
}

#[test]
fn key_cloned_exists_at_start() {
    let input = util::Source::new(vec![
        Poll::Ready(MapDiff::Replace {
            entries: vec![(1, 1), (2, 1), (3, 2), (4, 3)]
        }),
        Poll::Pending,
        Poll::Ready(MapDiff::Update {
            key: 1,
            value: 0,
        }),
        Poll::Ready(MapDiff::Update {
            key: 2,
            value: 0,
        }),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(MapDiff::Remove {key: 1})
    ]);

    let output = input.key_cloned(1);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(Some(1))),
        Poll::Ready(Some(Some(0))),
        Poll::Pending,
        Poll::Ready(Some(None)),
        Poll::Ready(None),
    ]);
}

#[test]
fn key_cloned_does_not_exist_at_start() {
    let input = util::Source::new(vec![
        Poll::Ready(MapDiff::Replace {
            entries: vec![(1, 1), (2, 1), (3, 2), (4, 3)]
        }),
        Poll::Pending,
        Poll::Ready(MapDiff::Insert {
            key: 5,
            value: 5,
        }),
        Poll::Ready(MapDiff::Update {
            key: 5,
            value: 0,
        }),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(MapDiff::Clear {})
    ]);

    let output = input.key_cloned(5);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(None)),
        Poll::Ready(Some(Some(0))),
        Poll::Pending,
        Poll::Ready(Some(None)),
        Poll::Ready(None),
    ]);
}

#[test]
fn key_cloned_empty() {
    let input = util::Source::<MapDiff<u32, u32>>::new(vec![]);

    let output = input.key_cloned(5);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(None)),
        Poll::Ready(None),
    ]);
}
