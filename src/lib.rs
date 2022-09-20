#![recursion_limit="128"]
#![warn(unreachable_pub)]
// missing_docs
#![deny(warnings, missing_debug_implementations, macro_use_extern_crate)]

///! It is *very highly* recommended to read the tutorial.
///! It explains all of the concepts you will need to use Signals effectively.

#[cfg(test)]
extern crate futures_executor;

// TODO should this be hidden from the docs ?
#[doc(hidden)]
#[macro_use]
pub mod internal;

#[doc(hidden)]
pub use gensym::gensym as __internal_gensym;

pub mod signal;
pub mod signal_vec;
pub mod signal_map;

mod atomic;
mod future;
pub use crate::future::{cancelable_future, CancelableFutureHandle, CancelableFuture};


/// # Tutorial
///
/// This tutorial is long, but it's intended to explain everything you need to
/// know in order to use Signals.
///
/// It is highly recommended to read through all of it.
///
/// ## `Mutable`
///
/// Before I can fully explain Signals, first I have to explain [`Mutable`](../signal/struct.Mutable.html):
///
/// ```rust
/// use futures_signals::signal::Mutable;
///
/// let my_state = Mutable::new(5);
/// ```
///
/// The above example creates a new `Mutable` with an initial value of `5`.
///
/// `Mutable` is very similar to [`Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html) + [`RwLock`](https://doc.rust-lang.org/std/sync/struct.RwLock.html):
///
/// * It implements [`Send`](https://doc.rust-lang.org/std/marker/trait.Send.html) and
///    [`Sync`](https://doc.rust-lang.org/std/marker/trait.Sync.html),
///    so it can be sent and used between multiple threads.
/// * It implements [`Clone`](https://doc.rust-lang.org/std/clone/trait.Clone.html),
///    which will create a new reference to the same `Mutable` (just like [`Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html)).
/// * You can retrieve the current value.
/// * You can change the current value.
///
/// Let's see it in action:
///
/// ```rust
/// # use futures_signals::signal::Mutable;
/// # let my_state = Mutable::new(5);
/// #
/// // Acquires a mutable lock on my_state
/// let mut lock = my_state.lock_mut();
///
/// assert_eq!(*lock, 5);
///
/// // Changes the current value of my_state to 10
/// *lock = 10;
///
/// assert_eq!(*lock, 10);
/// ```
///
/// ## `for_each`
///
/// However, if that was all `Mutable` could do, it wouldn't be very useful,
/// because `RwLock` already exists!
///
/// The major difference between `Mutable` and `RwLock` is that it is possible
/// to be efficiently notified whenever the `Mutable` changes:
///
/// ```rust
/// # use futures_signals::signal::Mutable;
/// # let my_state = Mutable::new(10);
/// #
/// use futures_signals::signal::SignalExt;
///
/// let future = my_state.signal().for_each(|value| {
///     // This code is run for the current value of my_state,
///     // and also every time my_state changes
///     println!("{}", value);
///     async {}
/// });
/// ```
///
/// This is how the [`for_each`](../signal/trait.SignalExt.html#method.for_each) method works:
///
/// 1. It returns a [`Future`](https://doc.rust-lang.org/std/future/trait.Future.html).
///
/// 2. When that [`Future`](https://doc.rust-lang.org/std/future/trait.Future.html)
///    is spawned, it will call the `|value| { ... }` closure with the *current
///    value* of `my_state` (which in this case is `10`).
///
/// 3. Then whenever `my_state` changes it will call the closure again with the new value.
///
/// This can be used for all sorts of things, such as automatically updating
/// your GUI or database whenever the `Signal` changes.
///
/// Just like [`Future`](https://doc.rust-lang.org/std/future/trait.Future.html) and
/// [`Stream`](https://docs.rs/futures/^0.3.15/futures/stream/trait.Stream.html),
/// when you create a `Signal` it does not actually do anything until it is spawned.
///
/// In order to spawn a `Signal` you first use the `for_each` method (as shown
/// above) to convert it into a `Future`, and then you spawn that `Future`.
///
/// There are many ways to spawn a `Future`:
///
/// * [`block_on(future)`](https://docs.rs/futures/^0.3.15/futures/executor/fn.block_on.html)
/// * [`tokio::spawn(future)`](https://docs.rs/tokio/^1.6.1/tokio/task/fn.spawn.html)
/// * [`task::spawn(future)`](https://docs.rs/async-std/^1.9.0/async_std/task/fn.spawn.html)
/// * [`spawn_local(future)`](https://docs.rs/wasm-bindgen-futures/^0.4.24/wasm_bindgen_futures/fn.spawn_local.html)
///
/// And many more! Because `for_each` returns a normal `Future`, anything that
/// can spawn a `Future` can also spawn a `Signal`.
///
/// That also means you can use all of the [`FutureExt`](https://docs.rs/futures/^0.3.15/futures/future/trait.FutureExt.html)
/// methods on it as well.
///
/// ## `to_stream`
///
/// If you need more control, you can use [`to_stream`](../signal/trait.SignalExt.html#method.to_stream) instead:
///
/// ```rust
/// # use futures_signals::signal::Mutable;
/// # let my_state = Mutable::new(10);
/// # use futures_signals::signal::SignalExt;
/// #
/// let stream = my_state.signal().to_stream();
/// ```
///
/// This returns a [`Stream`](https://docs.rs/futures/^0.3.15/futures/stream/trait.Stream.html)
/// of values (starting with the current value of `my_state`, and then followed
/// by the changes to `my_state`).
///
/// You can use all of the [`StreamExt`](https://docs.rs/futures/^0.3.15/futures/stream/trait.StreamExt.html)
/// methods on it, just like with any other
/// [`Stream`](https://docs.rs/futures/^0.3.15/futures/stream/trait.Stream.html).
///
/// ## `signal`
///
/// You might be wondering why you have to call the `signal()` method: why can't
/// you just use the `Mutable` directly?
///
/// There are actually three different methods, which you can use depending on
/// your needs:
///
/// 1. [`signal()`](../signal/struct.ReadOnlyMutable.html#method.signal) is the
///    most convenient but it requires the value to be [`Copy`](https://doc.rust-lang.org/std/marker/trait.Copy.html).
///
/// 2. [`signal_cloned()`](../signal/struct.ReadOnlyMutable.html#method.signal_cloned)
///    requires the value to be [`Clone`](https://doc.rust-lang.org/std/clone/trait.Clone.html).
///
/// 3. [`signal_ref(|x| { ... })`](../signal/struct.ReadOnlyMutable.html#method.signal_ref)
///    gives the closure a `&` reference to the value, so the value doesn't need
///    to be [`Copy`](https://doc.rust-lang.org/std/marker/trait.Copy.html) or
///    [`Clone`](https://doc.rust-lang.org/std/clone/trait.Clone.html), but
///    instead the value is determined by the closure.
///
///    This is the most flexible and also the fastest, but it is the longest and
///    most cumbersome to use.
///
/// In addition, it is possible to call the `signal` methods multiple times
/// (and mix-and-match them):
///
/// ```rust
/// # use futures_signals::signal::Mutable;
/// # let my_state = Mutable::new(10);
/// #
/// let signal1 = my_state.signal();
/// let signal2 = my_state.signal();
/// let signal3 = my_state.signal_cloned();
/// let signal4 = my_state.signal_ref(|x| *x + 10);
/// ```
///
/// When the `Mutable` changes, *all* of its `Signal`s are notified.
///
/// This turns out to be very useful in practice: it's common to put your
/// program's state inside of a global `Mutable` (or multiple `Mutable`s) and
/// then share it in various places throughout your program.
///
/// Lastly, there is a big difference between `Mutable` and `Signal`:
/// a `Signal` can only be notified when its value changes. However, a `Mutable`
/// can do a lot more than that, because it can retrieve the current value, and
/// it can also set the value.
///
/// ## `Signal`
///
/// Now that I've fully explained `Mutable`, I can finally explain [`Signal`](../signal/trait.Signal.html).
///
/// A `Signal` is an efficient zero-cost value which changes over time, and you
/// can be efficiently notified when it changes.
///
/// Just like [`Future`](https://doc.rust-lang.org/std/future/trait.Future.html)
/// and [`Stream`](https://docs.rs/futures/^0.3.15/futures/stream/trait.Stream.html),
/// `Signal`s are inlined and compiled into a very efficient state machine.
/// Most of the time they are fully stack allocated (*no* heap allocation).
///
/// And in the rare cases that they heap allocate they only do it *once*, when
/// the `Signal` is created, not while the `Signal` is running. This means the
/// performance is excellent, even with millions of `Signal`s running simultaneously.
///
/// Just like [`FutureExt`](https://docs.rs/futures/^0.3.15/futures/future/trait.FutureExt.html) and
/// [`StreamExt`](https://docs.rs/futures/0.3.15/futures/stream/trait.StreamExt.html),
/// the [`SignalExt`](../signal/trait.SignalExt.html) trait has many useful
/// methods, and most of them return a `Signal` so they can be chained:
///
/// ```rust
/// # use std::future::Future;
/// # use futures_signals::signal::Mutable;
/// # use futures_signals::signal::SignalExt;
/// # fn do_some_async_calculation(value: u32) -> impl Future<Output = ()> { async {} }
/// # fn main() {
/// # let my_state = Mutable::new(3);
/// #
/// let output = my_state.signal()
///     .map(|value| value + 5)
///     .map_future(|value| do_some_async_calculation(value))
///     .dedupe();
/// # }
/// ```
///
/// Let's say that the current value of `my_state` is `10`.
///
/// When `output` is spawned it will call the `|value| value + 5` closure with
/// the current value of `my_value` (the closure returns `10 + 5`, which is `15`).
///
/// Then it calls `do_some_async_calculation(15)` which returns a `Future`.
/// When that `Future` finishes, `dedupe` checks if the return value is
/// different from the previous value (using `==`), and if so then `output`
/// notifies with the new value.
///
/// It automatically repeats this process whenever `my_state` changes, ensuring
/// that `output` is always kept in sync with `my_state`.
///
/// It is also possible to use [`map_ref`](../macro.map_ref.html) which is similar
/// to [`map`](../signal/trait.SignalExt.html#method.map) except it allows you
/// to use multiple input `Signal`s:
///
/// ```rust
/// # use futures_signals::signal::Mutable;
/// # fn main() {
/// # let foo = Mutable::new(1);
/// # let bar = Mutable::new(2);
/// # let qux = Mutable::new(3);
/// #
/// use futures_signals::map_ref;
///
/// let output = map_ref! {
///     let foo = foo.signal(),
///     let bar = bar.signal(),
///     let qux = qux.signal() =>
///     *foo + *bar + *qux
/// };
/// # }
/// ```
///
/// In this case `map_ref` is depending on three `Signal`s: `foo`, `bar`, and `qux`.
///
/// Whenever any of those `Signal`s changes, it will rerun the `*foo + *bar + *qux`
/// code.
///
/// This means that `output` will always be equal to the sum of `foo`, `bar`, and `qux`.
///
/// ## Signals are lossy
///
/// It is important to understand that `for_each`, `to_stream`, and *all*
/// other `SignalExt` methods are *lossy*: they might skip changes.
///
/// That is because they only care about the *most recent value*. So if the
/// value changes multiple times in a short period of time it will only detect
/// the most recent change.
///
/// Here is an example:
///
/// ```rust
/// # use futures_signals::signal::Mutable;
/// # let my_state = Mutable::new(10);
/// #
/// my_state.set(2);
/// my_state.set(3);
/// ```
///
/// In this case it will only detect the `3` change. The `2` change is
/// completely ignored, like as if it never happened.
///
/// This is an intentional design choice: it is necessary for correctness and
/// performance.
///
/// So whenever you are using a `Signal`, you must ***not*** rely on it being
/// updated for intermediate values.
///
/// That might sound like a problem, but it's actually not a problem at all:
/// `Signal`s are guaranteed to always be updated with the most recent value,
/// so it's *only* intermediate values which aren't guaranteed.
///
/// This is similar to `RwLock`, which also does not give you access to past
/// values (only the current value). So as long as your program only cares about
/// the most recent value, then `Signal`s will work perfectly.
///
/// If you really *do* need all intermediate values (not just the most recent),
/// then using a [`Stream`](https://docs.rs/futures/^0.3.15/futures/stream/trait.Stream.html)
/// (such as [`futures::channel::mpsc::unbounded`](https://docs.rs/futures/^0.3.15/futures/channel/mpsc/fn.unbounded.html))
/// would be a great choice. In that case you will pay a small performance
/// penalty, because it has to hold the values in a queue.
///
/// And of course you can freely mix `Future`s, `Stream`s, and `Signal`s in
/// your program, using each one where it is appropriate.
///
/// ## `MutableVec`
///
/// In addition to `Mutable` and `Signal`, there is also [`MutableVec`](../signal_vec/struct.MutableVec.html),
/// [`SignalVec`](../signal_vec/trait.SignalVec.html), and [`SignalVecExt`](../signal_vec/trait.SignalVecExt.html).
///
/// As its name suggests, `MutableVec<A>` is very similar to `Mutable<Vec<A>>`,
/// except it's *dramatically* more efficient: rather than being notified with
/// the new `Vec`, instead you are notified with the *difference* between the
/// old `Vec` and the new `Vec`.
///
/// Here is an example:
///
/// ```rust
/// use futures_signals::signal_vec::MutableVec;
///
/// let my_vec: MutableVec<u32> = MutableVec::new();
/// ```
///
/// The above creates a new empty `MutableVec`.
///
/// You can then use [`lock_mut`](../signal_vec/struct.MutableVec.html#method.lock_mut),
/// which returns a lock. As its name implies, while you are holding the lock
/// you have exclusive mutable access to the `MutableVec`.
///
/// The lock contains many of the `Vec` methods:
///
/// ```rust
/// # use futures_signals::signal_vec::MutableVec;
/// # let my_vec: MutableVec<u32> = MutableVec::new();
/// #
/// let mut lock = my_vec.lock_mut();
/// lock.push(1);
/// lock.insert(0, 2);
/// lock.remove(0);
/// lock.pop().unwrap();
/// // And a lot more!
/// ```
///
/// The insertion methods require `Copy`, but there are also `_cloned` variants
/// which require `Clone` instead:
///
/// ```rust
/// # use std::sync::Arc;
/// # use futures_signals::signal_vec::MutableVec;
/// # let my_vec: MutableVec<u32> = MutableVec::new();
/// # let mut lock = my_vec.lock_mut();
/// #
/// lock.push_cloned(1);
/// lock.insert_cloned(0, 2);
/// // etc.
/// ```
///
/// This is needed because when you insert a new value, it has to send a copy to
/// all of the `SignalVec`s which are listening to changes.
///
/// The lock also has a `Deref` implementation for `&[T]`, so you can use *all*
/// of the immutable [`slice`](https://doc.rust-lang.org/std/primitive.slice.html)
/// methods on it:
///
/// ```rust
/// # use futures_signals::signal_vec::MutableVec;
/// # let my_vec: MutableVec<u32> = MutableVec::new_with_values(vec![0]);
/// # let lock = my_vec.lock_mut();
/// #
/// let _ = lock[0];
/// let _ = lock.len();
/// let _ = lock.last();
/// let _ = lock.iter();
/// // And a lot more!
/// ```
///
/// Lastly, you can use the [`signal_vec()`](../signal_vec/struct.MutableVec.html#method.signal_vec)
/// or [`signal_vec_cloned()`](../signal_vec/struct.MutableVec.html#method.signal_vec_cloned)
/// methods to convert it into a `SignalVec`, and then you can use the
/// [`for_each`](../signal_vec/trait.SignalVecExt.html#method.for_each) method
/// to be efficiently notified when it changes:
///
/// ```rust
/// # use futures_signals::signal_vec::MutableVec;
/// # let my_vec: MutableVec<u32> = MutableVec::new();
/// #
/// use futures_signals::signal_vec::{SignalVecExt, VecDiff};
///
/// let future = my_vec.signal_vec().for_each(|change| {
///     match change {
///         VecDiff::Replace { values } => {
///             // ...
///         },
///         VecDiff::InsertAt { index, value } => {
///             // ...
///         },
///         VecDiff::UpdateAt { index, value } => {
///             // ...
///         },
///         VecDiff::RemoveAt { index } => {
///             // ...
///         },
///         VecDiff::Move { old_index, new_index } => {
///             // ...
///         },
///         VecDiff::Push { value } => {
///             // ...
///         },
///         VecDiff::Pop {} => {
///             // ...
///         },
///         VecDiff::Clear {} => {
///             // ...
///         },
///     }
///
///     async {}
/// });
/// ```
///
/// Just like `SignalExt::for_each`, the `SignalVecExt::for_each` method returns
/// a `Future`.
///
/// When that `Future` is spawned:
///
/// 1. If the `SignalVec` already has values, it calls the closure with
///    `VecDiff::Replace`, which contains the current values for the `SignalVec`.
///
/// 2. If the `SignalVec` doesn't have any values, it doesn't call the closure.
///
/// 3. Whenever the `SignalVec` changes, it calls the closure with the `VecDiff`
///    for the change.
///
/// Unlike `SignalExt::for_each`, the `SignalVecExt::for_each` method calls the
/// closure with a [`VecDiff`](../signal_vec/enum.VecDiff.html), which contains
/// the difference between the new `Vec` and the old `Vec`.
///
/// As an example, if you call `lock.push(5)`, then the closure will be called with `VecDiff::Push { value: 5 }`
///
/// And if you call `lock.insert(3, 10)`, then the closure will be called with `VecDiff::InsertAt { index: 3, value: 10 }`
///
/// This allows you to very efficiently update based only on that specific change.
///
/// For example, if you are automatically saving the `MutableVec` to a database
/// whenever it changes, you don't need to save the entire `MutableVec` when it
/// changes, you only need to save the individual changes. This means that it will
/// often be constant `O(1)` time, no matter how big the `MutableVec` is.
///
/// ## `SignalVecExt`
///
/// Just like `SignalExt`, [`SignalVecExt`](../signal_vec/trait.SignalVecExt.html)
/// has a lot of useful methods, and most of them return a `SignalVec` so they
/// can be chained:
///
/// ```rust
/// # use futures_signals::signal_vec::MutableVec;
/// # let my_vec: MutableVec<u32> = MutableVec::new();
/// # use futures_signals::signal_vec::SignalVecExt;
/// #
/// let output = my_vec.signal_vec()
///     .filter(|value| *value < 5)
///     .map(|value| value + 10);
/// ```
///
/// They are generally very efficient (e.g. `map` is constant time, no matter
/// how big the `SignalVec` is, and `filter` is linear time).
///
/// ## SignalVec is lossless
///
/// Unlike `Signal`, it is guaranteed that the `SignalVec` will never skip a
/// change. In addition, the changes will always be in the correct order.
///
/// This is because it is notifying with the difference between the old `Vec`
/// and the new `Vec`, so it is very important that it is in the correct order,
/// and that it doesn't skip anything!
///
/// That does mean that `MutableVec` needs to maintain a queue of changes, so
/// this has a minor performance cost.
///
/// But because it's so efficient to update based upon the difference between
/// the old and new `Vec`, it's still often faster to use `MutableVec<A>` rather
/// than `Mutable<Vec<A>>`, even with the extra performance overhead.
///
/// In addition, even though `MutableVec` needs to maintain a queue, `SignalVec`
/// does ***not***, so it's quite efficient.
///
/// If you call a `MutableVec` method which doesn't *actually* make any changes,
/// then it will not notify at all:
///
/// ```rust
/// # use futures_signals::signal_vec::MutableVec;
/// # let my_vec: MutableVec<u32> = MutableVec::new();
/// #
/// my_vec.lock_mut().retain(|_| { true });
/// ```
///
/// The [`MutableVec::retain`](../signal_vec/struct.MutableVecLockMut.html#method.retain)
/// method is the same as [`Vec::retain`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.retain),
/// it calls the closure for each value in the `MutableVec`, and if the closure
/// returns `false` it removes that value from the `MutableVec`.
///
/// But in the above example, it never returns `false`, so it never removes
/// anything, so it doesn't notify.
///
/// Also, even though it's guaranteed to send a notification for each change,
/// the notification might be different than what you expect.
///
/// For example, when calling the `retain` method, it will send out a
/// notification for each change, so if `retain` removes 5 values it will send
/// out 5 notifications.
///
/// But, contrary to what you might expect, the notifications are in the reverse
/// order: it sends notifications for the right-most values first, and
/// notifications for the left-most values last. In addition, it sends a mixture
/// of `VecDiff::Pop` and `VecDiff::RemoveAt`.
///
/// Another example is that [`remove`](../signal_vec/struct.MutableVecLockMut.html#method.remove)
/// might notify with either `VecDiff::RemoveAt` or `VecDiff::Pop` depending on
/// whether it removed the last value or not.
///
/// The reason for this is performance, and you should ***not*** rely
/// upon it: the behavior of exactly which notifications are sent is an
/// implementation detail.
///
/// However, there is one thing you *can* rely on: if you apply the
/// notifications in the same order they are received, it will exactly recreate
/// the `SignalVec`:
///
/// ```rust
/// # use futures_signals::signal_vec::MutableVec;
/// # let my_vec: MutableVec<u32> = MutableVec::new();
/// # use futures_signals::signal_vec::{SignalVecExt, VecDiff};
/// #
/// let mut copied_vec = vec![];
///
/// let future = my_vec.signal_vec().for_each(move |change| {
///     match change {
///         VecDiff::Replace { values } => {
///             copied_vec = values;
///         },
///         VecDiff::InsertAt { index, value } => {
///             copied_vec.insert(index, value);
///         },
///         VecDiff::UpdateAt { index, value } => {
///             copied_vec[index] = value;
///         },
///         VecDiff::RemoveAt { index } => {
///             copied_vec.remove(index);
///         },
///         VecDiff::Move { old_index, new_index } => {
///             let value = copied_vec.remove(old_index);
///             copied_vec.insert(new_index, value);
///         },
///         VecDiff::Push { value } => {
///             copied_vec.push(value);
///         },
///         VecDiff::Pop {} => {
///             copied_vec.pop().unwrap();
///         },
///         VecDiff::Clear {} => {
///             copied_vec.clear();
///         },
///     }
///
///     async {}
/// });
/// ```
///
/// In the above example, `copied_vec` is guaranteed to have exactly the same
/// values as `my_vec`, in the same order as `my_vec`.
///
/// But even though the *end result* is guaranteed to be the same, the order of
/// the individual changes is an unspecified implementation detail.
///
/// ## `MutableBTreeMap`
///
/// Just like how `MutableVec` is a `Signal` version of `Vec`, there is also
/// [`MutableBTreeMap`](../signal_map/struct.MutableBTreeMap.html) which is a
/// `Signal` version of [`BTreeMap`](https://doc.rust-lang.org/std/collections/struct.BTreeMap.html):
///
/// ```rust
/// use futures_signals::signal_map::MutableBTreeMap;
///
/// let map = MutableBTreeMap::new();
///
/// let mut lock = map.lock_mut();
/// lock.insert("foo", 5);
/// lock.insert("bar", 10);
/// ```
///
/// Similar to `MutableVec`, it notifies with a [`MapDiff`](../signal_map/enum.MapDiff.html),
/// and of course it supports [`SignalMap`](../signal_map/trait.SignalMap.html)
/// and [`SignalMapExt`](../signal_map/trait.SignalMapExt.html) for efficient
/// transformations and notifications:
///
/// ```rust
/// # use futures_signals::signal_map::MutableBTreeMap;
/// # let map: MutableBTreeMap<&str, u32> = MutableBTreeMap::new();
/// #
/// use futures_signals::signal_map::{SignalMapExt, MapDiff};
///
/// let output = map.signal_map().for_each(|change| {
///     match change {
///         MapDiff::Replace { entries } => {
///             // ...
///         },
///         MapDiff::Insert { key, value } => {
///             // ...
///         },
///         MapDiff::Update { key, value } => {
///             // ...
///         },
///         MapDiff::Remove { key } => {
///             // ...
///         },
///         MapDiff::Clear {} => {
///             // ...
///         },
///     }
///
///     async {}
/// });
/// ```
///
/// ## End
///
/// And that's the end of the tutorial! We didn't cover every method, but we
/// covered enough for you to get started.
///
/// You can look at the documentation for information on every method (there's a
/// lot of useful stuff in there!).
pub mod tutorial {}
