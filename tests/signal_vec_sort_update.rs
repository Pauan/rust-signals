use std::pin::Pin;
use std::task::Poll;

use futures_executor::block_on;
use futures_util::future::poll_fn;

use futures_signals::signal::Mutable;
use futures_signals::signal_vec::{MutableVec, SignalVec};
use futures_signals::signal_vec::SignalVecExt;

#[test]
fn signal_vec_sort_update() {
    let mutable_vec = MutableVec::new_with_values(vec![
        Mutable::new("X".to_string()),
        Mutable::new("Y".to_string()),
        Mutable::new("Z".to_string()),
    ]);

    let mut signal_vec = mutable_vec
        .signal_vec_cloned()
        .sort_by_cloned(move |left: &Mutable<String>, right: &Mutable<String>| {
            left.get_cloned().cmp(&right.get_cloned())
        });

    let mut diffs = move || {
        block_on(poll_fn(|context| {
            let mut output = Vec::new();
            loop {
                let poll = Pin::new(&mut signal_vec).poll_vec_change(context);
                return match poll {
                    Poll::Ready(Some(item)) => {
                        output.push(item);
                        continue;
                    }
                    _ => {
                        Poll::Ready(output)
                    }
                };
            }
        }))
    };

    assert_eq!(format!("{:?}", diffs()), r#"[Replace { values: [Mutable("X"), Mutable("Y"), Mutable("Z")] }]"#);

    let last_item = mutable_vec.lock_mut().remove(2);
    assert_eq!(format!("{:?}", last_item), r#"Mutable("Z")"#);

    last_item.set("A".to_string());
    assert_eq!(format!("{:?}", last_item), r#"Mutable("A")"#);

    mutable_vec.lock_mut().push_cloned(last_item);
    assert_eq!(format!("{:?}", diffs()), "UNREACHABLE");
}

