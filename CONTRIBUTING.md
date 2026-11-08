# Contributing

## Start without hardware

```sh
cargo run -p tapboxd -- sim     # play with the box in a terminal
cargo test --workspace          # unit tests, property tests, card vectors, scenarios
```

Rust is pinned in `rust-toolchain.toml`; `rustup` picks it up automatically.

## Where things go

| You want to change… | Look in |
|---|---|
| How cards are encoded/decoded | `crates/tapbox-card` (and `docs/card-spec.md`, plus a vector in `card-spec/vectors/`) |
| What the box *decides* (resume, priority, gestures, volume) | `crates/tapbox-core` — pure logic, add a unit test in `core_tests.rs` |
| How the box *does* things (files, network, audio, hardware) | `crates/tapboxd` |
| A behaviour you can describe as "tap this, press that, expect this" | Add a scenario in `testbench/scenarios/` |

The core never does I/O and never reads the clock. If a change needs either, it belongs in `tapboxd`, and the core gets an `Event` or emits a `Command`.

## Before you push

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same, plus `cargo deny check` and a cross build for the Pi.

## Rules of thumb

- No `unsafe` (forbidden workspace-wide).
- No `unwrap`/`expect`/`panic!` outside tests. Card data is untrusted: a panic is a crash loop on a kid's speaker.
- Newtypes over primitives where values can be confused (`Uid`, `ContentHash`, `Percent`).
- Public items have doc comments (`missing_docs` is on).
- New dependencies: pure Rust if possible (keeps the static aarch64 build simple), and a licence `deny.toml` allows.

## Licence

By contributing you agree your work is dual-licensed under MIT and Apache-2.0, like the rest of the code.
