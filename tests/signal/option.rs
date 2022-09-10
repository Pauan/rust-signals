use std::task::Poll;
use futures_signals::signal::{SignalExt, Always, option};
use crate::util;


#[test]
fn test_option() {
    let mut signal = option::<Always<()>>(None);

    util::with_noop_context(|cx| {
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(Some(None)));
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(None));
    });
}

#[test]
fn test_option_signal() {
    let test_value = 0;

    let input = match test_value {
        0 => Some(util::Source::new(vec![
            Poll::Ready(0),
            Poll::Pending,
            Poll::Ready(3),
            Poll::Pending,
        ])),
        _ => None,
    };

    let output = option(input);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(Some(0))),
        Poll::Pending,
        Poll::Ready(Some(Some(3))),
        Poll::Pending,
        Poll::Ready(None),
    ]);
}
