#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_result {
    ($cx:ident, $name:ident = $pat:pat,) => {
        $name.as_mut().poll($cx)
    };
    ($cx:ident, $name:ident = $pat:pat, $($args:tt)+) => {
        $name.as_mut().poll($cx).merge($crate::__internal_map_result!($cx, $($args)+))
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_pin {
    ($($name:ident = $pat:pat,)+) => {
        $(let mut $name = $name.unsafe_pin();)+
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_value_ref {
    ($($name:ident = $pat:pat,)+) => {
        $(let $pat = $name.value_ref();)+
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_value_mut {
    ($($name:ident = $pat:pat,)+) => {
        $(let $pat = $name.value_mut();)+
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_identifier {
    ($gensym:ident, $macro:ident, { $($bindings:tt)* }, let $name:pat = $value:expr, $($rest:tt)+) => {{
        let mut $gensym = $crate::internal::MapRef1::new($value);

        $crate::__internal_map!($macro, { $($bindings)* $gensym = $name, }, $($rest)+)
    }};

    ($gensym:ident, $macro:ident, { $($bindings:tt)* }, let $name:pat = $value:expr => $($rest:tt)+) => {{
        let mut $gensym = $crate::internal::MapRef1::new($value);

        $crate::__internal_map!($macro, { $($bindings)* $gensym = $name, }, => $($rest)+)
    }};

    ($gensym:ident, $macro:ident, { $($bindings:tt)* }, $name:ident, $($rest:tt)+) => {{
        let mut $gensym = $crate::internal::MapRef1::new($name);

        $crate::__internal_map!($macro, { $($bindings)* $gensym = $name, }, $($rest)+)
    }};

    ($gensym:ident, $macro:ident, { $($bindings:tt)* }, $name:ident => $($rest:tt)+) => {{
        let mut $gensym = $crate::internal::MapRef1::new($name);

        $crate::__internal_map!($macro, { $($bindings)* $gensym = $name, }, => $($rest)+)
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map {
    // This is only included for backwards compatibility
    // TODO remove in next major version
    ($macro:ident, { $($bindings:tt)* }, => move $f:expr) => {
        $crate::__internal_map!($macro, { $($bindings)* }, => $f)
    };

    ($macro:ident, { $($bindings:tt)* }, => $f:expr) => {
        $crate::internal::MapRefSignal::new(move |cx| {
            $crate::__internal_map_pin!($($bindings)*);

            let result = $crate::__internal_map_result!(cx, $($bindings)+);

            if result.changed {
                $crate::$macro!($($bindings)*);

                ::std::task::Poll::Ready(Some($f))

            } else if result.done {
                ::std::task::Poll::Ready(None)

            } else {
                ::std::task::Poll::Pending
            }
        })
    };

    ($($rest:tt)*) => {
        $crate::__internal_gensym!($crate::__internal_identifier!($($rest)*))
    };
}


/// `map_mut` is exactly the same as `map_ref`, except it gives
/// *mutable* references (`map_ref` gives *immutable* references).
///
/// `map_mut` is almost never useful, so it's recommended to use
/// `map_ref` instead.
#[macro_export]
macro_rules! map_mut {
    ($($input:tt)*) => {
        $crate::__internal_map!(__internal_value_mut, {}, $($input)*)
    };
}


/// The `map_ref` macro can be used to *combine* multiple `Signal`s together:
///
/// ```rust
/// # use futures_signals::map_ref;
/// # use futures_signals::signal::Mutable;
/// # fn main() {
/// #
/// let mutable1 = Mutable::new(1);
/// let mutable2 = Mutable::new(2);
///
/// let output = map_ref! {
///     let value1 = mutable1.signal(),
///     let value2 = mutable2.signal() =>
///     *value1 + *value2
/// };
/// # }
/// ```
///
/// In the above example, `map_ref` takes two input Signals: `mutable1.signal()` and `mutable2.signal()`,
/// and it returns an output Signal.
///
/// When the output Signal is spawned:
///
/// 1. It takes the current value of `mutable1.signal()` and puts it into the `value1` variable.
///
/// 2. It takes the current value of `mutable2.signal()` and puts it into the `value2` variable.
///
/// 3. Then it runs the `*value1 + *value2` code, and puts the result of that code into the output Signal.
///
/// 4. Whenever `mutable1.signal()` or `mutable2.signal()` changes it repeats the above steps.
///
/// So the end result is that `output` always contains the value of `mutable1 + mutable2`.
///
/// So in the above example, `output` will have the value `3` (because it's `1 + 2`).
///
/// But let's say that `mutable1` changes...
///
/// ```rust
/// # use futures_signals::signal::Mutable;
/// # let mutable1 = Mutable::new(1);
/// #
/// mutable1.set(5);
/// ```
///
/// ...then `output` will now have the value `7` (because it's `5 + 2`). And then if `mutable2` changes...
///
/// ```rust
/// # use futures_signals::signal::Mutable;
/// # let mutable2 = Mutable::new(2);
/// #
/// mutable2.set(10);
/// ```
///
/// ...then `output` will now have the value `15` (because it's `5 + 10`).
///
/// If multiple input Signals change at the same time, then it will only update once:
///
/// ```rust
/// # use futures_signals::signal::Mutable;
/// # let mutable1 = Mutable::new(5);
/// # let mutable2 = Mutable::new(10);
/// #
/// mutable1.set(15);
/// mutable2.set(20);
/// ```
///
/// In the above example, `output` will now have the value `35` (because it's `15 + 20`), and it only
/// updates once (***not*** once per input Signal).
///
/// ----
///
/// There is also a shorthand syntax:
///
/// ```rust
/// # use futures_signals::map_ref;
/// # use futures_signals::signal::always;
/// # fn main() {
/// # let signal1 = always(1);
/// # let signal2 = always(2);
/// #
/// let output = map_ref!(signal1, signal2 => *signal1 + *signal2);
/// # }
/// ```
///
/// The above code is exactly the same as this:
///
/// ```rust
/// # use futures_signals::map_ref;
/// # use futures_signals::signal::always;
/// # fn main() {
/// # let signal1 = always(1);
/// # let signal2 = always(2);
/// #
/// let output = map_ref! {
///     let signal1 = signal1,
///     let signal2 = signal2 =>
///     *signal1 + *signal2
/// };
/// # }
/// ```
///
/// This only works if the input Signals are variables. If you want to use expressions for the input
/// Signals then you must either assign them to variables first, or you must use the longer syntax.
///
/// In addition, it's possible to use pattern matching with the longer syntax:
///
/// ```rust
/// # use futures_signals::map_ref;
/// # use futures_signals::signal::always;
/// # fn main() {
/// # struct SomeStruct { foo: u32 }
/// # let signal1 = always((1, 2));
/// # let signal2 = always(SomeStruct { foo: 3 });
/// #
/// let output = map_ref! {
///     let (t1, t2) = signal1,
///     let SomeStruct { foo } = signal2 =>
///     // ...
/// #   ()
/// };
/// # }
/// ```
///
/// It's also possible to combine more than two Signals:
///
/// ```rust
/// # use futures_signals::map_ref;
/// # use futures_signals::signal::Mutable;
/// # fn main() {
/// # let mutable1 = Mutable::new(1);
/// # let mutable2 = Mutable::new(2);
/// # let mutable3 = Mutable::new(3);
/// #
/// let output = map_ref! {
///     let value1 = mutable1.signal(),
///     let value2 = mutable2.signal(),
///     let value3 = mutable3.signal() =>
///     *value1 + *value2 + *value3
/// };
/// # }
/// ```
///
/// You can combine an *infinite* number of Signals, there is no limit.
///
/// However, keep in mind that each input Signal has a small performance cost.
/// The cost is ***very*** small, but it grows linearly with the number of input Signals.
///
/// You shouldn't normally worry about it, just don't put thousands of input Signals
/// into a `map_ref` (this basically *never* happens in practice).
///
/// ----
///
/// You might be wondering why it's called `map_ref`: that's because `value1` and `value2` are *immutable `&` references*
/// to the current values of the input Signals. That's also why you need to use `*value1` and `*value2` to dereference them.
///
/// Why does it use references? Let's say one of the input Signals changes but the other ones haven't changed. In that situation
/// it needs to use the old values for the Signals that didn't change. But because that situation might happen multiple times,
/// it needs to retain ownership of the values, so it can only give out references.
///
/// Rather than giving out references, it could instead have been designed so it always
/// [`clone`](https://doc.rust-lang.org/std/clone/trait.Clone.html#tymethod.clone)s the values, but that's expensive
/// (and it means that it only works with types that implement [`Clone`](https://doc.rust-lang.org/std/clone/trait.Clone.html)).
///
/// Because [`clone`](https://doc.rust-lang.org/std/clone/trait.Clone.html#tymethod.clone) only requires an immutable
/// reference, it's easy to call [`clone`](https://doc.rust-lang.org/std/clone/trait.Clone.html#tymethod.clone) yourself
/// when you need to:
///
/// ```rust
/// # use futures_signals::map_ref;
/// # use futures_signals::signal::Mutable;
/// # fn main() {
/// # let mutable1 = Mutable::new(1);
/// # let mutable2 = Mutable::new(2);
/// #
/// let output = map_ref! {
///     let value1 = mutable1.signal(),
///     let value2 = mutable2.signal() =>
///     value1.clone() + value2.clone()
/// };
/// # }
/// ```
///
/// So because it gives references, you can now manually call [`clone`](https://doc.rust-lang.org/std/clone/trait.Clone.html#tymethod.clone)
/// (or any other `&self` method) *only* when you need to. This improves performance.
///
/// # Performance
///
/// Everything is stack allocated, there are no heap allocations, performance is optimal.
///
/// Because it must poll all of the input Signals, the performance is proportional to the
/// number of Signals. However, polling is ***very*** fast.
#[macro_export]
macro_rules! map_ref {
    ($($input:tt)*) => {
        $crate::__internal_map!(__internal_value_ref, {}, $($input)*)
    };
}
