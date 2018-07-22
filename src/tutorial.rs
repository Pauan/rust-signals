///! # Tutorial
///!
///! This tutorial is long, but it's intended to explain everything you need to know in order to use Signals.
///!
///! It is highly recommended to read through all of it.
///!
///! Before I can fully explain Signals, first I have to explain `Mutable`:
///!
///! ```rust
///! use futures_signals::signal::Mutable;
///!
///! let my_state = Mutable::new(5);
///! ```
///!
///! The above example creates a new `Mutable` with an initial value of `5`.
///!
///! `Mutable` is very similar to [`RwLock`](https://doc.rust-lang.org/std/sync/struct.RwLock.html):
///!
///! * It implements [`Send`](https://doc.rust-lang.org/std/marker/trait.Send.html) and [`Sync`](https://doc.rust-lang.org/std/marker/trait.Sync.html), so it can be sent and used between multiple threads.
///! * You can retrieve the current value.
///! * You can change the current value.
///!
///! Let's see it in action:
///!
///! ```rust
///! # use futures_signals::signal::Mutable;
///! # let my_state = Mutable::new(5);
///! #
///! assert_eq!(my_state.get(), 5);
///!
///! // Changes the current value to 10
///! my_state.set(10);
///!
///! assert_eq!(my_state.get(), 10);
///! ```
///!
///! However, if that was all `Mutable` could do, it wouldn't be very useful, because `RwLock`
///! already exists!
///!
///! The major difference between `Mutable` and `RwLock` is that it is possible to be
///! efficiently notified whenever the `Mutable` changes:
///!
///! ```rust
///! # use futures_signals::signal::Mutable;
///! # let my_state = Mutable::new(10);
///! #
///! use futures_signals::signal::SignalExt;
///!
///! let future = my_state.signal().for_each(|value| {
///!     // This code is run for the current value of my_state, and also every time my_state changes
///!     println!("{}", value);
///!     Ok(())
///! });
///! #
///! # use futures_signals::signal::ForEach;
///! # let future: ForEach<_, Result<(), ()>, _> = future;
///! ```
///!
///! This is how the `for_each` method works:
///!
///! 1. The `for_each` method returns a new [`Future`](https://docs.rs/futures/0.2.*/futures/trait.Future.html).
///!
///! 2. When that [`Future`](https://docs.rs/futures/0.2.*/futures/trait.Future.html) is spawned it will *immediately*
///!    call the `|value| { ... }` closure with the *current value* of `my_state` (which in this case is `10`).
///!
///! 3. Then whenever `my_state` changes (such as with `my_state.set(...)`) it will call the closure again with the new value.
///!
///! Just like [`Future`](https://docs.rs/futures/0.2.*/futures/trait.Future.html) and [`Stream`](https://docs.rs/futures/0.2.*/futures/trait.Stream.html),
///! when you create a `Signal` it does not actually do anything until it is spawned.
///!
///! In order to spawn a `Signal` you first use the `for_each` method (as shown above) to convert it into a `Future`, and then you spawn that `Future`.
///!
///! There are many ways of spawning a `Future`:
///!
///! * [`block_on(future)`](https://docs.rs/futures/0.2.*/futures/executor/fn.block_on.html)
///! * [`tokio::run(future)`](https://docs.rs/tokio/0.1.5/tokio/runtime/fn.run.html)
///! * `PromiseFuture::spawn_local(future)`
///!
///! And many more! Since `for_each` returns a normal [`Future`](https://docs.rs/futures/0.2.*/futures/trait.Future.html),
///! any [`Executor`](https://docs.rs/futures/0.2.*/futures/executor/trait.Executor.html) should work.
///!
///! That also means that you can use all of the [`FutureExt`](https://docs.rs/futures/0.2.*/futures/trait.FutureExt.html) methods on it as well.
///!
///! ----
///!
///! If you need more control, you can use `to_stream` instead:
///!
///! ```rust
///! # use futures_signals::signal::Mutable;
///! # let my_state = Mutable::new(10);
///! # use futures_signals::signal::SignalExt;
///! #
///! let stream = my_state.signal().to_stream();
///! ```
///!
///! This returns a [`Stream`](https://docs.rs/futures/0.2.*/futures/trait.Stream.html) of values (starting with the current value of `my_state`, and
///! then followed by the changes to `my_state`).
///!
///! You can then use all of the [`StreamExt`](https://docs.rs/futures/0.2.*/futures/trait.StreamExt.html) methods on it, just like with any other
///! [`Stream`](https://docs.rs/futures/0.2.*/futures/trait.Stream.html).
///!
///! ----
///!
///! You might be wondering why you have to call the `signal` method: why can't you just use the
///! `Mutable` directly?
///!
///! There's three reasons:
///!
///! 1. Because `SignalExt` methods like `for_each` consume their input, that would mean that after
///!    calling `for_each` on a `Mutable` you would no longer be able to change the `Mutable`, which
///!    defeats the whole point of using `Mutable` in the first place!
///!
///! 2. It is possible to call `signal` multiple times:
///!
///!    ```rust
///!    # use futures_signals::signal::Mutable;
///!    # let my_state = Mutable::new(10);
///!    #
///!    let signal1 = my_state.signal();
///!    let signal2 = my_state.signal();
///!    ```
///!
///!    When the `Mutable` changes, *all* of its Signals are notified.
///!
///!    This turns out to be very useful in practice: it's common to put your program's state inside
///!    of a global `Mutable` (or multiple `Mutable`s) and then share it in various places throughout your
///!    program.
///!
///! 3. You cannot be notified when a `Mutable` changes, but you can get/set its current value.
///!
///!    On the other hand, you *can* be notified when a `Signal` changes, but you cannot get/set
///!    the current value of the `Signal`.
///!
///!    This split is necessary both for correctness and performance. Therefore, because of this
///!    split, it is necessary to call the `signal` method to "convert" a `Mutable` into a `Signal`.
///!
///! ----
///!
///! It is important to understand that `for_each`, `to_stream`, and *all* other `Signal` methods
///! are *lossy*: they might skip changes.
///!
///! That is because they only care about the *most recent value*. So if the value changes
///! multiple times in a short period of time it will only detect the most recent change.
///!
///! Here is an example:
///!
///! ```rust
///! # use futures_signals::signal::Mutable;
///! # let my_state = Mutable::new(10);
///! #
///! my_state.set(2);
///! my_state.set(3);
///! ```
///!
///! In this case it will only detect the `3` change. The `2` change is completely ignored,
///! like as if it never happened.
///!
///! This is an intentional design choice: it is necessary for correctness and performance.
///!
///! So whenever you are using `Signal`, you must ***not*** rely upon it being updated for intermediate
///! values.
///!
///! That might sound like a problem, but it's actually not a problem at all: it ***is*** guaranteed that it
///! will be updated with the most recent value, it is *only* intermediate values that aren't guaranteed.
///!
///! This is similar to `RwLock`, which does not give you access to past values (only the current value),
///! and the same is true with `Mutable` and `Signal`.
///!
///! If you really *do* need all intermediate values (not just the most recent), then using a
///! [`Stream`](https://docs.rs/futures/0.2.*/futures/trait.Stream.html) would be a great choice.
///! In that case you will pay a small performance penalty, because it has to hold the values in a queue.
///!
///! ----
///!
///! Now that I've fully explained `Mutable`, I can finally explain `Signal`.
///!
///! Just like [`Future`](https://docs.rs/futures/0.2.*/futures/trait.FutureExt.html) and
///! [`Stream`](https://docs.rs/futures/0.2.*/futures/trait.StreamExt.html), `Signal` has many useful
///! methods, and most of them return a `Signal` so they can be chained:
///!
///! ```rust
///! # extern crate futures_core;
///! # extern crate futures_signals;
///! # use futures_core::Never;
///! # use futures_signals::signal::Mutable;
///! # use futures_signals::signal::SignalExt;
///! # fn do_some_async_calculation(value: u32) -> Result<(), Never> { Ok(()) }
///! # fn main() {
///! # let my_state = Mutable::new(3);
///! #
///! let mapped = my_state.signal()
///!     .map(|value| value + 5)
///!     .map_future(|value| do_some_async_calculation(value));
///! # }
///! ```
///!
///! ----
///!
///! In addition to `Mutable` and `Signal`, there is also `MutableVec` and `SignalVec`.
///!
///! As its name suggests, `MutableVec<A>` is very similar to `Mutable<Vec<A>>`, except it's *dramatically*
///! more efficient: rather than being notified with the new `Vec`, instead you are notified with the *difference*
///! between the old `Vec` and the new `Vec`.
///!
///! Here is an example:
///!
///! ```rust
///! # use futures_signals::signal_vec::MutableVec;
///! #
///! let my_vec: MutableVec<u32> = MutableVec::new();
///! ```
///!
///! The above creates a new empty `MutableVec`.
///!
///! You can then use `lock_mut`, which returns a lock. As its name implies, while you are holding the lock
///! you have exclusive access to the `MutableVec`.
///!
///! The lock contains many of the `Vec` methods:
///!
///! ```rust
///! # use futures_signals::signal_vec::MutableVec;
///! # let my_vec: MutableVec<u32> = MutableVec::new();
///! #
///! let mut lock = my_vec.lock_mut();
///! lock.push(1);
///! lock.insert(0, 2);
///! lock.remove(0);
///! lock.pop().unwrap();
///! // And a lot more!
///! ```
///!
///! It also has a `Deref` implementation for `&[T]`, so you can use *all* of the [`slice`](https://doc.rust-lang.org/std/primitive.slice.html) methods on it:
///!
///! ```rust
///! # use futures_signals::signal_vec::MutableVec;
///! # let my_vec: MutableVec<u32> = MutableVec::new_with_values(vec![0]);
///! # let lock = my_vec.lock_mut();
///! #
///! let _ = lock[0];
///! let _ = lock.len();
///! let _ = lock.last();
///! let _ = lock.iter();
///! // And a lot more!
///! ```
///!
///! Lastly, you can use the `MutableVec::signal_vec` method to convert it into a `SignalVec`, and then you can use the
///! `for_each` method to be efficiently notified when it changes:
///!
///! ```rust
///! # use futures_signals::signal_vec::MutableVec;
///! # let my_vec: MutableVec<u32> = MutableVec::new();
///! #
///! use futures_signals::signal_vec::{SignalVecExt, VecDiff};
///!
///! let future = my_vec.signal_vec().for_each(|change| {
///!     match change {
///!         VecDiff::Replace { values } => {
///!             // ...
///!         },
///!         VecDiff::InsertAt { index, value } => {
///!             // ...
///!         },
///!         VecDiff::UpdateAt { index, value } => {
///!             // ...
///!         },
///!         VecDiff::RemoveAt { index } => {
///!             // ...
///!         },
///!         VecDiff::Move { old_index, new_index } => {
///!             // ...
///!         },
///!         VecDiff::Push { value } => {
///!             // ...
///!         },
///!         VecDiff::Pop {} => {
///!             // ...
///!         },
///!         VecDiff::Clear {} => {
///!             // ...
///!         },
///!     }
///!
///!     Ok(())
///! });
///! #
///! # use futures_signals::signal_vec::ForEach;
///! # let future: ForEach<_, Result<(), ()>, _> = future;
///! ```
///!
///! Just like `Signal::for_each`, the `SignalVec::for_each` method returns a `Future`.
///!
///! When that `Future` is spawned:
///!
///! 1. If the `SignalVec` already has values, it immediately calls the closure with `VecDiff::Replace`,
///!    which contains the current values for the `SignalVec`.
///!
///! 2. If the `SignalVec` doesn't have any values, it doesn't call the closure.
///!
///! 3. Whenever the `SignalVec` changes, it calls the closure with the `VecDiff` for the change.
///!
///! Unlike `Signal::for_each`, the `SignalVec::for_each` method calls the closure with a `VecDiff`, which contains
///! the difference between the new `Vec` and the old `Vec`.
///!
///! As an example, if you call `my_vec.push(5)`, then the closure will be called with `VecDiff::Push { value: 5 }`
///!
///! And if you call `my_vec.insert(3, 10)`, then the closure will be called with `VecDiff::InsertAt { index: 3, value: 10 }`
///!
///! This allows you to very efficiently update based only on that specific change.
///!
///! For example, if you are automatically saving the `MutableVec` to a database whenever it changes, you don't need to save the
///! entire `MutableVec` when it changes, you only need to save the individual change. This means that it will often be constant
///! time, no matter how big the `MutableVec` is.
///!
///! ----
///!
///! Unlike `Signal`, it is guaranteed that the `SignalVec` will never skip a change. In addition, the changes will always
///! be in the correct order.
///!
///! This is because it is notifying with the difference between the old `Vec` and the new `Vec`, so it is very important that
///! it is in the correct order, and that it doesn't skip anything!
///!
///! That does mean that `MutableVec` needs to maintain a queue of changes, so this has a minor performance cost.
///!
///! But because it's so efficient to update based upon the difference between the old and new `Vec`, it's still often faster
///! to use `MutableVec<A>` rather than `Mutable<Vec<A>>`, even with the extra performance overhead.
///!
///! In addition, even though `MutableVec` needs to maintain a queue, `SignalVec` does ***not***, so it's quite efficient.
///!
///! Even though it does not skip changes, if you call a `MutableVec` method which doesn't *actually* make any changes, then it will
///! not notify at all:
///!
///! ```rust
///! # use futures_signals::signal_vec::MutableVec;
///! # let my_vec: MutableVec<u32> = MutableVec::new();
///! #
///! my_vec.lock_mut().retain(|_| { true });
///! ```
///!
///! The `MutableVec::retain` method is the same as [`Vec::retain`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.retain):
///! it calls the closure with each value in the `MutableVec`, and if the closure returns `false` it then removes that
///! value from the `MutableVec`.
///!
///! But in the above example, it never returns `false`, so it never removes anything, so it doesn't notify.
///!
///! Also, even though it's guaranteed to send a notification for each change, the notification might be different than what you expect.
///!
///! For example, when calling the `retain` method, it will send out a notification for each change, so if `retain` removes 5 values it will send
///! out 5 notifications.
///!
///! But, contrary to what you might expect, the notifications are in the reverse order: it sends notifications for the right-most values
///! first, and notifications for the left-most values last. In addition, it sends a mixture of `VecDiff::Pop` and `VecDiff::RemoveAt`.
///!
///! Another example is that `my_vec.remove(index)` might notify with either `VecDiff::RemoveAt` or `VecDiff::Pop` depending on whether
///! `index` is the last index or not.
///!
///! The reason this is done is for performance, and you should ***not*** rely upon it: the behavior of exactly which notifications are
///! sent is an implementation detail.
///!
///! The only thing you can rely upon is that if you apply the notifications in the same order they are received, it will exactly recreate the
///! `SignalVec`:
///!
///! ```rust
///! # use futures_signals::signal_vec::MutableVec;
///! # let my_vec: MutableVec<u32> = MutableVec::new();
///! # use futures_signals::signal_vec::{SignalVecExt, VecDiff};
///! #
///! let mut copied_vec = vec![];
///!
///! let future = my_vec.signal_vec().for_each(move |change| {
///!     match change {
///!         VecDiff::Replace { values } => {
///!             copied_vec = values;
///!         },
///!         VecDiff::InsertAt { index, value } => {
///!             copied_vec.insert(index, value);
///!         },
///!         VecDiff::UpdateAt { index, value } => {
///!             copied_vec[index] = value;
///!         },
///!         VecDiff::RemoveAt { index } => {
///!             copied_vec.remove(index);
///!         },
///!         VecDiff::Move { old_index, new_index } => {
///!             let value = copied_vec.remove(old_index);
///!             copied_vec.insert(new_index, value);
///!         },
///!         VecDiff::Push { value } => {
///!             copied_vec.push(value);
///!         },
///!         VecDiff::Pop {} => {
///!             copied_vec.pop().unwrap();
///!         },
///!         VecDiff::Clear {} => {
///!             copied_vec.clear();
///!         },
///!     }
///!
///!     Ok(())
///! });
///! #
///! # use futures_signals::signal_vec::ForEach;
///! # let future: ForEach<_, Result<(), ()>, _> = future;
///! ```
///!
///! In the above example, `copied_vec` is guaranteed to always have exactly the same values as `my_vec`, in the same order as `my_vec`.
///!
///! But even though the *end result* is guaranteed to be the same, the order of the individual changes is an unspecified implementation detail.
///!
///! ----
///!
///! Just like `Signal`, `SignalVec` has a lot of useful methods, and most of them return a `SignalVec` so they can be chained:
///!
///! ```rust
///! # use futures_signals::signal_vec::MutableVec;
///! # let my_vec: MutableVec<u32> = MutableVec::new();
///! # use futures_signals::signal_vec::SignalVecExt;
///! #
///! let filter_mapped = my_vec.signal_vec()
///!     .filter(|value| *value < 5)
///!     .map(|value| value + 10);
///! ```
///!
///! ----
///!
///! And that's the end of the tutorial! We didn't cover every method, but we covered enough for you to get started.
///!
///! You can look at the documentation for information on every method (there's a lot of useful stuff in there!).

// TODO figure out a way to remove this
#[allow(unused_imports)]
use std;
