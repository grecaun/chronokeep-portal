# Patched copy of i2c-character-display 0.6.0

This is i2c-character-display 0.6.0 from crates.io with one fix, used through
`[patch.crates-io]` in the portal's Cargo.toml.

## The fix

`src/driver/hd44780/adapter.rs`, `adapter_init()` (lines marked `PATCHED`):

- Waits 50ms before initializing, since the HD44780 needs more than 40ms after power-on.
- Waits 2ms after the clear display and return home commands. They take at least 1.52ms,
  and the crate sent the next command straight away. When the first half of that command
  reached the controller while it was busy, it was dropped, and the display stayed out of
  step showing garbage until the next init. It happened only on some boots and no I2C
  errors were reported.

Once a release fixes it, remove this directory and the
`[patch.crates-io]` entry in Cargo.toml.
