This crate provides zero-cost Signals which are built on top of the
[futures](https://crates.io/crates/futures) crate.

What is a Signal? It is a *value that changes over time*, and you can be
efficiently notified whenever its value changes.

This is useful in many situations: you can automatically serialize your program's
state to a database whenever it changes, or you can use dominator to automatically update
the DOM whenever your program's state changes, etc.

Before I can fully explain Signals, first I have to explain `Mutable`:

```rust
let my_state = Mutable::new(5);
```

The above example creates a new `Mutable` with an initial value of `5`. You can think
of `Mutable` as being very similar to `RwLock`:

* It implements `Send` and `Sync`
* You can retrieve the current value
* You can change the current value

Let's see it in action:

```rust
println!("{}", my_state.get()); // prints 5
my_state.set(10);
println!("{}", my_state.get()); // prints 10
```

However, if that was all `Mutable` could do, it wouldn't be very useful, since `RwLock`
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

To explain in more detail: the `for_each` method returns a new `Future`. When that `Future` is
spawned it will *immediately* call the `|value| { ... }` closure with the *current value* of
`my_state` (which in this case is `10`), and then whenever `my_state` changes
(e.g. with `my_state.set(...)`) it will call the closure again with the new value.

Because the `for_each` method returns a `Future`, you need to spawn it somehow. There are
many ways of doing so:

* `block_on(future)`
* `tokio::run(future)`
* `PromiseFuture::spawn_local(future)`

And many more! Any Executor should work, since `for_each` is a normal `Future`.

That also means that you can use all of the `FutureExt` methods on it as well.

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

* Because `Future` methods like `for_each` consume their input, that would mean that
  after calling `for_each` on a `Mutable` you would no longer be able to change the
  `Mutable`, which defeats the whole point of using `Mutable` in the first place!

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
`Future` and `Stream` support various useful methods, `Signal` also contains many useful
methods.

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
