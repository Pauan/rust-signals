[![crates.io](http://meritbadge.herokuapp.com/futures-signals)](https://crates.io/crates/futures-signals)
[![docs.rs](https://docs.rs/futures-signals/badge.svg)](https://docs.rs/futures-signals)

This is a Rust crate that provides zero-cost Signals which are built on top of the
[futures](https://crates.io/crates/futures) crate.

Hold on, zero-cost? Yup, that's right: if you don't use a feature you don't pay any performance cost,
and the features that you *do* use are as fast as possible. Signals are ***very*** efficient.

What is a Signal? It is a *value that changes over time*, and you can be efficiently
notified whenever its value changes.

This is useful in many situations:

* You can automatically serialize your program's state to a database whenever it changes.

* You can automatically send a message to the server whenever the client's state changes, or vice versa. This
  can be used to automatically, efficiently, and conveniently keep the client and server's state in sync.

* A game engine can use Signals to automatically update the game's state whenever something changes.

* You can easily represent continuous input (such as the current temperature, or the current time) as a Signal.

* If you create a GUI, you can use Signals to automatically update the GUI whenever your state changes, ensuring
  that your state and the GUI are always in sync.

* You can use [dominator](https://crates.io/crates/dominator) to create web apps and automatically keep them in
  sync with your program's state.

* And many more situations!

The best way to learn more is to read [the tutorial](https://docs.rs/futures-signals/^0.3.21/futures_signals/tutorial/index.html).
