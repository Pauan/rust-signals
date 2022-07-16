use std::sync::Mutex;
use std::task::Poll;
use indoc::formatdoc;
use once_cell::sync::Lazy;
use futures_signals::signal_vec::{SignalVecExt, VecDiff};

mod util;

#[test]
fn test_debug_signal_vec() {
    struct VecLogger {
        messages: Mutex<Vec<String>>,
    }

    impl VecLogger {
        fn new() -> Self {
            Self {
                messages: Mutex::new(vec![]),
            }
        }

        fn messages(&self) -> Vec<String> {
            self.messages.lock().unwrap().clone()
        }
    }

    impl log::Log for VecLogger {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                self.messages.lock().unwrap().push(std::fmt::format(*record.args()));
            }
        }

        fn flush(&self) {}
    }


    static LOGGER: Lazy<VecLogger> = Lazy::new(|| VecLogger::new());

    log::set_logger(&*LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Trace);


    let input = util::Source::<VecDiff<u32>>::new(vec![
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

    let output = input.debug();

    let changes = util::map_poll_vec(output, |_output, change| change);

    assert_eq!(changes, vec![
        Poll::Ready(Some(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] })),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(VecDiff::InsertAt { index: 0, value: 6 })),
        Poll::Ready(Some(VecDiff::InsertAt { index: 2, value: 7 })),
        Poll::Pending,
        Poll::Ready(Some(VecDiff::RemoveAt { index: 0 })),
        Poll::Ready(Some(VecDiff::UpdateAt { index: 4, value: 0 })),
        Poll::Pending,
        Poll::Ready(Some(VecDiff::Move { old_index: 1, new_index: 3 })),
        Poll::Pending,
        Poll::Ready(Some(VecDiff::RemoveAt { index: 1 })),
        Poll::Pending,
        Poll::Ready(Some(VecDiff::Clear {})),
        Poll::Ready(Some(VecDiff::Replace { values: vec![] })),
        Poll::Ready(None),
    ]);

    let location = "tests/debug.rs:66:24";

    assert_eq!(LOGGER.messages(), vec![
        formatdoc! {"
            [{location}] Ready(
                Some(
                    Replace {{
                        values: [
                            0,
                            1,
                            2,
                            3,
                            4,
                            5,
                        ],
                    }},
                ),
            )"},
        formatdoc! {"
            [{location}] Pending"},
        formatdoc! {"
            [{location}] Pending"},
        formatdoc! {"
            [{location}] Ready(
                Some(
                    InsertAt {{
                        index: 0,
                        value: 6,
                    }},
                ),
            )"},
        formatdoc! {"
            [{location}] Ready(
                Some(
                    InsertAt {{
                        index: 2,
                        value: 7,
                    }},
                ),
            )"},
        formatdoc! {"
            [{location}] Pending"},
        formatdoc! {"
            [{location}] Ready(
                Some(
                    RemoveAt {{
                        index: 0,
                    }},
                ),
            )"},
        formatdoc! {"
            [{location}] Ready(
                Some(
                    UpdateAt {{
                        index: 4,
                        value: 0,
                    }},
                ),
            )"},
        formatdoc! {"
            [{location}] Pending"},
        formatdoc! {"
            [{location}] Ready(
                Some(
                    Move {{
                        old_index: 1,
                        new_index: 3,
                    }},
                ),
            )"},
        formatdoc! {"
            [{location}] Pending"},
        formatdoc! {"
            [{location}] Ready(
                Some(
                    RemoveAt {{
                        index: 1,
                    }},
                ),
            )"},
        formatdoc! {"
            [{location}] Pending"},
        formatdoc! {"
            [{location}] Ready(
                Some(
                    Clear,
                ),
            )"},
        formatdoc! {"
            [{location}] Ready(
                Some(
                    Replace {{
                        values: [],
                    }},
                ),
            )"},
        formatdoc! {"
            [{location}] Ready(
                None,
            )"},
    ]);
}
