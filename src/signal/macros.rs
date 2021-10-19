#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_mut_pairs {
    (let $name1:pat = $value1:expr; let $name2:pat = $value2:expr;) => {
        $crate::internal::MapPairMut::new($value1, $value2)
    };
    (let $name:pat = $value:expr; $($args:tt)+) => {
        $crate::internal::MapPairMut::new($value, $crate::__internal_map_mut_pairs!($($args)+))
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_mut_borrows {
    ($f:expr, $r:ident, { $($lets:stmt)* }, let $name:pat = $value:expr;) => {
        {
            $($lets;)*
            // TODO is this correct ?
            let mut $r = $crate::internal::lock_mut(&(*$r).1);
            let $name = $crate::internal::unwrap_mut(&mut $r);
            $f
        }
    };
    ($f:expr, $r:ident, { $($lets:stmt)* }, let $name:pat = $value:expr; $($args:tt)+) => {
        $crate::__internal_map_mut_borrows!(
            $f,
            $r,
            {
                $($lets)*
                // TODO is this correct ?
                let mut $r = $crate::internal::lock_mut(&(*$r).1)
                let $r = $crate::internal::unwrap_mut(&mut $r)
                // TODO is this correct ?
                let mut l = $crate::internal::lock_mut(&(*$r).0)
                let $name = $crate::internal::unwrap_mut(&mut l)
            },
            $($args)+
        )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_mut {
    (($($move:tt)*), $f:expr, let $name:pat = $value:expr;) => {
        $crate::signal::SignalExt::map($value, $($move)* |mut x| {
            let $name = &mut x;
            $f
        })
    };
    (($($move:tt)*), $f:expr,
        let $name1:pat = $value1:expr;
        let $name2:pat = $value2:expr;
    ) => {
        $crate::internal::Map2::new($value1, $value2, $($move)* |$name1, $name2| $f)
    };
    (($($move:tt)*), $f:expr,
        let $name1:pat = $value1:expr;
        let $name2:pat = $value2:expr;
        $($args:tt)+
    ) => {
        $crate::internal::Map2::new(
            $value1,
            $crate::__internal_map_mut_pairs!(let $name2 = $value2; $($args)+),
            $($move)* |$name1, r| $crate::__internal_map_mut_borrows!(
                $f,
                r,
                {
                    // TODO is this correct ?
                    let mut l = $crate::internal::lock_mut(&r.0)
                    let $name2 = $crate::internal::unwrap_mut(&mut l)
                },
                $($args)+
            )
        )
    };
}

/*let mut s = a2.map2(b2.map_pair(c2.map_pair(d2.map_pair(e2))), |a: &mut Cloner, r: &mut Pair<u32, Pair<u32, Pair<u32, Cloner>>>| {
    let a: &mut Cloner = a;

    let mut l = r.0.borrow_mut();
    let b: &mut u32 = l.as_mut().unwrap();

    let mut r = r.1.borrow_mut();
    let r = r.as_mut().unwrap();
    let mut l = r.0.borrow_mut();
    let c: &mut u32 = l.as_mut().unwrap();

    let mut r = r.1.borrow_mut();
    let r = r.as_mut().unwrap();
    let mut l = r.0.borrow_mut();
    let d: &mut u32 = l.as_mut().unwrap();

    let mut r = r.1.borrow_mut();
    let e: &mut Cloner = r.as_mut().unwrap();

    (a.clone(), *b, *c, *d, e.clone())
});*/

/// `map_mut` is exactly the same as `map_ref`, except it gives
/// *mutable* references (`map_ref` gives *immutable* references).
///
/// `map_mut` is almost never useful, and it's a little slower than
/// `map_ref`, so it's ***highly*** recommended to use `map_ref` instead.
#[macro_export]
macro_rules! map_mut {
    ($($input:tt)*) => { $crate::__internal_map_split!(__internal_map_mut, (), $($input)*) };
}


#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_ref_pairs {
    (let $name1:pat = $value1:expr; let $name2:pat = $value2:expr;) => {
        $crate::internal::MapPair::new($value1, $value2)
    };
    (let $name:pat = $value:expr; $($args:tt)+) => {
        $crate::internal::MapPair::new($value, $crate::__internal_map_ref_pairs!($($args)+))
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_ref_borrows {
    ($f:expr, $r:ident, { $($lets:stmt)* }, let $name:pat = $value:expr;) => {
        {
            $($lets;)*
            let $name = $crate::internal::unwrap_ref(&(*$r).1);
            $f
        }
    };
    ($f:expr, $r:ident, { $($lets:stmt)* }, let $name:pat = $value:expr; $($args:tt)+) => {
        $crate::__internal_map_ref_borrows!(
            $f,
            $r,
            {
                $($lets)*
                // TODO is this correct ?
                let $r = $crate::internal::lock_ref(&$crate::internal::unwrap_ref(&(*$r).1))
                let $name = $crate::internal::unwrap_ref(&(*$r).0)
            },
            $($args)+
        )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_ref {
    (($($move:tt)*), $f:expr, let $name:pat = $value:expr;) => {
        $crate::signal::SignalExt::map($value, $($move)* |x| {
            let $name = &x;
            $f
        })
    };
    (($($move:tt)*), $f:expr,
        let $name1:pat = $value1:expr;
        let $name2:pat = $value2:expr;
    ) => {
        $crate::internal::Map2::new($value1, $value2, $($move)* |x, y| {
            // TODO is there a better way of converting from &mut to & ?
            let $name1 = &*x;
            let $name2 = &*y;
            $f
        })
    };
    (($($move:tt)*), $f:expr,
        let $name1:pat = $value1:expr;
        let $name2:pat = $value2:expr;
        $($args:tt)+
    ) => {
        $crate::internal::Map2::new(
            $value1,
            $crate::__internal_map_ref_pairs!(let $name2 = $value2; $($args)+),
            $($move)* |l, r| $crate::__internal_map_ref_borrows!(
                $f,
                r,
                {
                    // TODO is there a better way of converting from &mut to & ?
                    let $name1 = &*l
                    // TODO is this correct ?
                    let r = $crate::internal::lock_ref(&r)
                    let $name2 = $crate::internal::unwrap_ref(&r.0)
                },
                $($args)+
            )
        )
    };
}

/*let mut s = a2.map2(b2.map_pair(c2.map_pair(d2.map_pair(e2))), |a, r| {
    let a: &Cloner = a;

    let r = r.borrow();
    let b: &u32 = r.0.as_ref().unwrap();

    let r = r.1.as_ref().unwrap().borrow();
    let c: &u32 = r.0.as_ref().unwrap();

    let r = r.1.as_ref().unwrap().borrow();
    let d: &u32 = r.0.as_ref().unwrap();

    let e: &Cloner = r.1.as_ref().unwrap();

    (a.clone(), *b, *c, *d, e.clone())
});*/

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
/// The cost is very small, but it grows linearly with the number of input Signals.
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
/// If you use 1 or 2 input Signals it is ***extremely*** fast, and everything is stack allocated.
///
/// If you use 3+ input Signals, it will do a heap allocation for each input Signal.
/// However, this heap allocation only happens once, when the `map_ref` is created.
/// It does *not* do any heap allocation while polling. So it's still ***very*** fast.
///
/// In addition, if you use 3+ input Signals, there is a *very* small additional performance
/// cost on every poll, and this additional cost scales linearly with the number of input
/// Signals.
#[macro_export]
macro_rules! map_ref {
    ($($input:tt)*) => { $crate::__internal_map_split!(__internal_map_ref, (), $($input)*) };
}


#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_lets {
    ($macro:ident, ($($move:tt)*), $f:expr, { $($lets:tt)* },) => {
        $crate::$macro!(($($move)*), $f, $($lets)*)
    };
    ($macro:ident, ($($move:tt)*), $f:expr, { $($lets:tt)* }, let $name:pat = $value:expr, $($args:tt)*) => {
        $crate::__internal_map_lets!($macro, ($($move)*), $f, { $($lets)* let $name = $value; }, $($args)*)
    };
    ($macro:ident, ($($move:tt)*), $f:expr, { $($lets:tt)* }, $name:ident, $($args:tt)*) => {
        $crate::__internal_map_lets!($macro, ($($move)*), $f, { $($lets)* let $name = $name; }, $($args)*)
    };
}

// TODO this is pretty inefficient, it iterates over the token tree one token at a time
#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_split {
    ($macro:ident, ($($before:tt)*), => move $f:expr) => {
        $crate::__internal_map_lets!($macro, (move), $f, {}, $($before)*,)
    };
    ($macro:ident, ($($before:tt)*), => $f:expr) => {
        $crate::__internal_map_lets!($macro, (), $f, {}, $($before)*,)
    };
    ($macro:ident, ($($before:tt)*), $t:tt $($after:tt)*) => {
        $crate::__internal_map_split!($macro, ($($before)* $t), $($after)*)
    };
}
