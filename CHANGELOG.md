## NEXT
* The `signals::channel` implementation is now lock-free (if the allocator is lock-free).
* `Broadcaster` now impls `Clone`.

## 0.3.24 - (2021-12-29)
* Changing `SignalExt::debug` to use the [`log` crate](https://crates.io/crates/log).
* Adding in new `signal_vec::from_stream` function.

## 0.3.23 - (2021-10-20)
* `Mutable::clone` is now lock-free.
* Adding in `SignalExt::debug` method which prints the state of the signal to the console.
* Fixing bug with `from_stream` function.

## 0.3.22 - (2021-06-13)
* Adding in `SignalExt::boxed` and `SignalExt::boxed_local` methods, for feature-parity with `FutureExt` and `StreamExt`.
* Adding in `Broadcaster::signal_ref` method.
* Fixing various bugs with `Broadcaster`.
