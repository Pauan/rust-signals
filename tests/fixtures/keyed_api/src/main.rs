use futures_signals::signal::Signal;
use futures_signals::signal_vec::{always, SignalVec, SignalVecExt, VecDiff};
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

#[derive(Clone)]
struct Value<'a>(&'a str);

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
struct Key(Rc<str>);

fn accept<'a, S, G>(_: S)
where
    S: SignalVec<Item = (Key, G)>,
    G: SignalVec<Item = Value<'a>>,
{
}

fn accept_borrowed<'a, S, G>(_: S)
where
    S: SignalVec<Item = (&'a str, G)>,
    G: SignalVec<Item = i32>,
{
}

struct PinnedSource {
    _pin: PhantomPinned,
}
impl SignalVec for PinnedSource {
    type Item = i32;
    fn poll_vec_change(self: Pin<&mut Self>, _: &mut Context) -> Poll<Option<VecDiff<i32>>> {
        Poll::Ready(None)
    }
}

struct PinnedKeys<'a> {
    text: &'a str,
    _pin: PhantomPinned,
}
impl<'a> Signal for PinnedKeys<'a> {
    type Item = Vec<&'a str>;
    fn poll_change(self: Pin<&mut Self>, _: &mut Context) -> Poll<Option<Self::Item>> {
        Poll::Ready(Some(vec![self.text]))
    }
}

fn main() {
    let text = String::from("member");
    let mut calls = 0;
    let source = always(vec![Value(text.as_str())]);
    accept(source.group_by_key(|value: &Value<'_>| {
        calls += 1;
        Key(Rc::from(value.0))
    }));
    assert_eq!(calls, 0);
    let mut factories = 0;
    accept(
        always(vec![Value(text.as_str())]).group_by_keys_signal(|value: &Value<'_>| {
            factories += 1;
            futures_signals::signal::always(vec![Key(Rc::from(value.0))])
        }),
    );
    assert_eq!(factories, 0);
    let borrowed = String::from("borrowed");
    let groups = PinnedSource {
        _pin: PhantomPinned,
    }
    .group_by_keys_signal(|_| PinnedKeys {
        text: borrowed.as_str(),
        _pin: PhantomPinned,
    });
    accept_borrowed(groups);

    let key_text = String::from("borrowed key");
    let groups = always(vec![()]).group_by_key(|_| key_text.as_str());
    drop(groups);

    let groups = PinnedSource {
        _pin: PhantomPinned,
    }
    .group_by_key(|value| *value);
    drop(groups);
}
