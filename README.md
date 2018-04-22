This crate provides zero-cost Signals which are built on top of the
[futures](https://crates.io/crates/futures) crate.

What is a Signal? It is a *value that changes over time*, and you can be efficiently
notified whenever its value changes.

This is useful in many situations:

* You can automatically serialize your program's state to a database whenever it changes.

* You can automatically send a message to the server whenever the client's state changes, or vice versa. This
  can be used to automatically, efficiently, and conveniently keep the client and server's state in sync.

* You can use [dominator](https://crates.io/crates/dominator) to automatically update the DOM whenever your
  program's state changes.

* And many more situations!

Before I can fully explain Signals, first I have to explain `Mutable`:

```rust
let my_state = Mutable::new(5);
```

The above example creates a new `Mutable` with an initial value of `5`.

`Mutable`  is very similar to `RwLock`:

* It implements `Send` and `Sync`.
* You can retrieve the current value.
* You can change the current value.

Let's see it in action:

```rust
println!("{}", my_state.get()); // prints 5

my_state.set(10);

println!("{}", my_state.get()); // prints 10
```

However, if that was all `Mutable` could do, it wouldn't be very useful, because `RwLock`
already exists!

The major difference between `Mutable` and `RwLock` is that it is possible to be
efficiently notified whenever the `Mutable` changes:

```rust
let future = my_state.signal().for_each(|value| {
    println!("{}", value);
    Ok(())
});
```

In this case we are using `my_state.signal().for_each(|value| { ... })` to be notified
whenever `my_state` changes.

Whenever I say "changes", what I really mean is "the current value, and also any changes to
the value in the future".

To explain in more detail:

1. The `for_each` method returns a new [`Future`](https://docs.rs/futures/0.2.*/futures/trait.Future.html).

2. When that [`Future`](https://docs.rs/futures/0.2.*/futures/trait.Future.html) is spawned it will *immediately* call the `|value| { ... }` closure with the *current value* of
`my_state` (which in this case is `10`).

3. Then whenever `my_state` changes (e.g. with `my_state.set(...)`) it will call the closure again with the new value.

Because the `for_each` method returns a [`Future`](https://docs.rs/futures/0.2.*/futures/trait.Future.html), you need to spawn it somehow. There are
many ways of doing so:

* [`block_on(future)`](https://docs.rs/futures/0.2.*/futures/executor/fn.block_on.html)
* [`tokio::run(future)`](https://docs.rs/tokio/0.1.5/tokio/runtime/fn.run.html)
* `PromiseFuture::spawn_local(future)`

And many more! Any [`Executor`](https://docs.rs/futures/0.2.*/futures/executor/trait.Executor.html) should work, since
`for_each` is a normal [`Future`](https://docs.rs/futures/0.2.*/futures/trait.Future.html).

That also means that you can use all of the [`FutureExt`](https://docs.rs/futures/0.2.*/futures/trait.FutureExt.html) methods on it as well.

If you need more control, you can use `to_stream` instead:

```rust
let stream = my_state.signal().to_stream();
```

This returns a `Stream` of values (starting with the current value of `my_state`, and
then followed by the changes to `my_state`).

You can then use all of the `StreamExt` methods on it, just like with any other `Stream`.

----

It is important to understand that `for_each`, `to_stream`, and other `Signal` methods
are *lossy*: they might not contain every change.

That is because they only care about the *most recent value*. So if the value changes
multiple times in a short time it will only detect the most recent change. Here is an
example:

```rust
my_state.set(2);
my_state.set(3);
```

In this case it will only detect the `3` change. The `2` change is completely ignored,
like as if it never happened.

This is an intentional design choice: it is necessary for correctness and performance.

In addition, it's not a problem in practice, because a `Signal` is supposed to represent
a *single value that changes over time*. Just like how `RwLock` does not give you
access to past values (only the current value), the same is true with `Mutable` and `Signal`.

So whenever you are using `Signal`, you must *not* rely upon them being updated for every
change. But you *can* rely upon them always containing the most recent value.

If you really do need every single intermediate value, then using a `Stream` would be
the correct choice. In that case you will pay a performance penalty, because it has to
hold the values in a queue.

----

At this point you might be wondering why you have to call the `signal` method: why
can't you just use the `Mutable` directly?

There's three reasons:

* Because [`FutureExt`](https://docs.rs/futures/0.2.*/futures/trait.FutureExt.html) methods
  like `for_each` consume their input, that would mean that after calling `for_each` on a
  `Mutable` you would no longer be able to change the `Mutable`, which defeats the whole point
  of using `Mutable` in the first place!

* It is possible to call `signal` multiple times:

  ```rust
  let signal1 = my_state.signal();
  let signal2 = my_state.signal();
  ```

  When the `Mutable` changes, *all* of its signals will change as well. This turns out
  to be very useful in practice: it's common to put your program's state inside of a
  `Mutable` (or multiple `Mutable`s) and then share it in various places.

* You cannot be notified when a `Mutable` changes, but you can get/set its current value.

  On the other hand, you *can* be notified when a `Signal` changes, but you cannot get/set
  the current value of the `Signal`.

  This split is necessary both for correctness and performance. Therefore, because of this
  split, it is necessary to call the `signal` method to "convert" a `Mutable` into a `Signal`.

----

Now that I've fully explained `Mutable`, I can finally explain `Signal`. Just like how
[`Future`](https://docs.rs/futures/0.2.*/futures/trait.FutureExt.html) and `Stream` support various
useful methods, `Signal` also contains many useful methods.

The most commonly used method is `map`:

```rust
// This contains the value of `my_state + 1`
let mapped = my_state.signal().map(|value| value + 1);
```

The `map` method takes an input `Signal` and a closure, and it returns an output `Signal`.

Whenever the input `Signal` changes, it calls the closure with the current value of the
input `Signal`, and then it updates the value of the output `Signal` with the return
value of the closure.

This updating happens *automatically and efficiently*, and it will call the closure at
most once for each change in `my_state`.

In the above example, `mapped` will always contain the current value of `my_state`, except
with `1` added to it. So if `my_state` is `10`, then `mapped` will be `11`. If `my_state`
is `5`, then `mapped` will be `6`, etc.

Also, like I explained earlier, whenever I say "changes" what I really mean is "the current
value, and then also any changes in the future".

And it's important to keep in mind that just like all of the `Signal` methods, `map` is
lossy: it might "skip" values. So you *cannot* rely upon the closure being called for every
value. But you *can* rely upon it always being called with the most recent value.

Because `map` returns a `Signal`, you can chain it with more `Signal` methods:

```rust
// This contains the value of `mapped + 1`, which is the same as `my_state + 2`
let mapped2 = mapped.map(|value| value + 1);
```

In the above example, `mapped2` contains the same value as `mapped`, except with `1` added
to it.

Because `mapped` contains the value of `my_state + 1`, that means `mapped2` is the same as
`my_state.signal().map(|value| value + 2)`

Lastly you can use `for_each` as usual to start listening to the changes:

```rust
let future = mapped.for_each(|value| { ... });
```

----

Another commonly used method is `map2`, except you shouldn't use it directly. Instead,
there are `map_ref` and `map_mut` macros (which internally use `map2`). Let's take a look:

```rust
let mutable1 = Mutable::new(1);
let mutable2 = Mutable::new(2);

let mapped = map_ref {
    let value1 = mutable1.signal(),
    let value2 = mutable2.signal() =>
    *value1 + *value2
};
```

The purpose of `map_ref` and `map_mut` is to *combine* multiple Signals together.

In the above example, `map_ref` takes two input Signals: `mutable1.signal()` and `mutable2.signal()`,
and it returns an output Signal.

It takes the current value of `mutable1.signal()` and puts it into the `value1` variable.
And it takes the current value of `mutable2.signal()` and puts it into the `value2` variable.
Then it runs the `*value1 + *value2` code, and puts the result of that code into the output Signal.

Whenever `mutable1.signal()` or `mutable2.signal()` changes, it will then repeat that process again:
it puts the current values of the signals into the `value1` and `value2` variables, then it runs the
`*value1 + *value2` code, and puts the result of that into the output Signal.

So the end result is that `mapped` always contains the value of `mutable1 + mutable2`. So in the above
example, `mapped` will be `3` (because it's `1 + 2`). But let's say that `mutable1` changes:

```rust
mutable1.set(5);
```

Now `mapped` will be `7` (because it's `5 + 2`). And then if `mutable2` changes...

```rust
mutable2.set(10);
```

...then `mapped` is now `15` (because it's `5 + 10`).

You might be wondering why it's called `map_ref`: that's because `value1` and `value2` are *immutable references*
to the current values of the Signals. That's also why you need to use `*value1` and `*value2` to dereference them.

This is the reason for why they are references: if one of the Signals changes but the other one hasn't, it needs to
use the old value for the Signal that didn't change. But because that situation might happen multiple times,
it needs to retain ownership of the value, so it can only give out references.

The only difference between `map_ref` and `map_mut` is that `map_ref` gives immutable references and `map_mut`
gives mutable references. `map_mut` is slightly slower than `map_ref`, and it's almost never useful to use `map_mut`,
so I recommend only using `map_ref`.

It's possible to combine more than two Signals:

```rust
let mapped = map_ref {
    let value1 = mutable1.signal(),
    let value2 = mutable2.signal(),
    let value3 = mutable3.signal() =>
    *value1 + *value2 + *value3
};
```

The `map_ref` and `map_mut` macros allow for an *infinite* number of Signals, there is no limit. However,
keep in mind that each Signal has a small performance cost. The cost is very small, but it grows linearly
with the number of Signals. You shouldn't normally worry about it, just don't put thousands of Signals
into a `map_ref` or `map_mut` (this basically *never* happens in practice).

----

In addition to `Mutable` and `Signal`, there is also `MutableVec` and `SignalVec`.

As its name suggests, `MutableVec<A>` is very similar to `Mutable<Vec<A>>`, except it's *dramatically*
more efficient.

Rather than being notified when the value changes, instead you are notified with the *difference* between
the old `Vec` and the new `Vec`. Here is an example:

```rust
let my_vec = MutableVec::new();
```

The above creates a new empty `MutableVec`. You can then use many of the `Vec` methods on it:

```rust
my_vec.push(1);
my_vec.insert(0, 2);
my_vec.remove(0);
my_vec.pop().unwrap();
```

In addition, you can use the `signal_vec` method to convert it into a `SignalVec`, and then you can use the
`for_each` method to be efficiently notified when it changes:

```rust
let future = my_vec.signal_vec().for_each(|change| {
    match change {
        VecChange::Replace { values } => { ... }
        VecChange::InsertAt { index, value } => { ... },
        VecChange::UpdateAt { index, value } => { ... },
        VecChange::RemoveAt { index } => { ... },
        VecChange::Move { old_index, new_index } => { ... },
        VecChange::Push { value } => { ... },
        VecChange::Pop {} => { ... },
        VecChange::Clear {} => { ... },
    }
});
```

Unlike `Signal`, the `for_each` method for `SignalVec` calls the closure with a `VecChange`, which contains
the difference between the new `Vec` and the old `Vec`.

As an example, if you call `my_vec.push(5)`, then the closure will be called with `VecChange::push { value: 5 }`

Or if you call `my_vec.insert(3, 10)`, then the closure will be called with `VecChange::InsertAt { index: 3, value: 10 }`

This allows you to very efficiently update based only on that specific change. For example, if you are automatically saving
the `MutableVec` to a database whenever it changes, you don't need to save the entire `MutableVec` when it changes, you only
need to save the individual change. This means that updates are often constant time, no matter how big the `MutableVec` is.

----

Unlike `Mutable` and `Signal`, it is guaranteed that the closure will be called with every single change: it will never "skip"
a change, and the changes will always be in the correct order.

This is because it is notifying with the difference between the old `Vec` and the new `Vec`, so it is very important that
it is in the correct order, and that it doesn't skip anything!

However, if you call a `MutableVec` method which doesn't *actually* make any changes, then it will not notify at all:

```rust
my_vec.retain(|_| { true });
```

The `MutableVec::retain` method is the same as `Vec::retain`: it calls the `|_| { true }` closure with each value in the `MutableVec`,
and if the closure returns `false` it then removes the value from the `MutableVec`.

But in the above example, it never returns `false`, so it never removes anything, so it doesn't notify.

Also, even though it's guaranteed to send a notification for each change, the notification might be different than what you expect.

For example, when calling the `retain` method, it will send out a notification for each change, so if `retain` removes 5 values it will send
out 5 notifications.

But the notifications are in the reverse order: it sends notifications for the right-most values first, and notifications
for the left-most values last. In addition, it sends a mixture of `VecChange::Pop` and `VecChange::RemoveAt` as appropriate.

Another example is that `my_vec.remove(index)` might notify with either `VecChange::RemoveAt` or `VecChange::Pop` depending on whether
`index` is the last index or not.

The reason this is done is for performance reasons, and you should ***not*** rely upon it: the behavior of exactly which notifications are
sent is an implementation detail.

The only thing you can rely upon is that if you apply the notifications in the same order they are received, it will exactly recreate the
`SignalVec`:

```rust
let mut copied_vec = vec![];

let future = my_vec.signal_vec().for_each(move |change| {
    match change {
        VecChange::Replace { values } => {
            *copied_vec = values;
        },
        VecChange::InsertAt { index, value } => {
            copied_vec.insert(index, value);
        },
        VecChange::UpdateAt { index, value } => {
            copied_vec[index] = value;
        },
        VecChange::RemoveAt { index } => {
            copied_vec.remove(index);
        },
        VecChange::Move { old_index, new_index } => {
            let value = copied_vec.remove(old_index);
            copied_vec.insert(new_index, value);
        },
        VecChange::Push { value } => {
            copied_vec.push(value);
        },
        VecChange::Pop {} => {
            copied_vec.pop().unwrap();
        },
        VecChange::Clear {} => {
            copied_vec.clear();
        },
    }
});
```

In the above example, `copied_vec` is guaranteed to have exactly the same values as `my_vec`.

----

Just like `Signal`, `SignalVec` has a lot of useful methods, and most of them return a `SignalVec` so they can be chained:

```rust
let filter_mapped = my_vec.signal_vec()
    .filter(|value| value < 5)
    .map(|value| value + 10);
```

----

A very common `SignalVec` method is `map`:

```rust
let mapped = my_vec.signal_vec().map(|value| value + 1);
```

The `map` method takes in an input `SignalVec` and a closure, and it returns an output `SignalVec`.

It calls the closure for each value in the input `SignalVec`, and the output `SignalVec` contains the
same values as the input `SignalVec`, except each value is replaced with the return value of the closure.

It is guaranteed that the closure will be called exactly once for each value in the input `SignalVec`.

When the input `SignalVec` changes, it automatically updates the output `SignalVec` as needed.

So in the above example, `mapped` is a `SignalVec` with the same values as `my_vec`, except with `1` added to them.

So if `my_vec` has the values `[1, 2, 3, 4, 5]` then `mapped` has the values `[2, 3, 4, 5, 6]`

This is a ***very*** efficient method: it is guaranteed constant time, regardless of how big the input `SignalVec` is.
The only exception is when the input `SignalVec` notifies with `VecChange::Replace`, in which case `map` is linear time.

----

Another common method is `filter`:

```rust
let filtered = my_vec.signal_vec().filter(|value| value < 5);
```

The `filter` method takes an input `SignalVec` and a closure, and it returns an output `SignalVec`.

It calls the closure for each value in the input `SignalVec`, and the output `SignalVec` only contains the
values where the closure returns `true`.

It is guaranteed that the closure will be called exactly once for each value in the input `SignalVec`.

When the input `SignalVec` changes, it automatically updates the output `SignalVec` as needed.

So in the above example, `filtered` is a `SignalVec` with the same values as `my_vec`, excluding the values that are greater than `4`.

So if `my_vec` has the values `[3, 1, 6, 2, 0, 4, 5, 8, 9, 7]` then `filtered` has the values `[3, 1, 2, 0, 4]`.

The performance is linear with the number of values in the input `SignalVec`. As an example, if you push a value into a `MutableVec`
which has 1,000 values, `filter` will take on average 1,000 operations to update its internal state. That might sound
expensive, but each individual operation is *very* fast, so it's normally not a problem unless you have a *huge* `SignalVec`.

----

Another common method is `sort_by_cloned`:

```rust
let sorted = my_vec.signal_vec().sort_by_cloned(|left, right| left.cmp(right));
```

The `sort_by_cloned` method takes an input `SignalVec` and a closure, and it returns an output `SignalVec`.

It calls the closure with two values from the input `SignalVec`, and the closure must return an [`Ordering`](https://doc.rust-lang.org/std/cmp/enum.Ordering.html), which is used to sort the values. The output `SignalVec` then contains the sorted values.

It automatically maintains the sort order even when the input `SignalVec` changes.

So in the above example, `sorted` is a `SignalVec` with the same values as `my_vec`, except sorted by `left.cmp(right)`.

So if `my_vec` has the values `[3, 1, 6, 2, 0, 4, 5, 8, 9, 7]` then `sorted` has the values `[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]`.

This method is intentionally very similar to the [`slice::sort_by`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by) method,
except it doesn't mutate the input `SignalVec` (it returns a new `SignalVec`).

The reason why it has the `_cloned` suffix is because it *clones* the values from the input `SignalVec`. This is
necessary in order to maintain its internal state while also simultaneously passing the values to the output `SignalVec`.

This method has the same logarithmic performance as [`slice::sort_by`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by),
except it's slower because it needs to keep track of extra internal state. As an example, if you insert a value into a `MutableVec` which
has 1,000 values, then `sort_by_cloned` will take on average ~2,010 operations to update its internal state.
