# Contributing to tapbox

Thanks for helping! tapbox is meant to be built by anyone: parents, makers, schools. Code, docs, enclosure designs, translations and test reports from real builds are all welcome.

## Ways to help

- **Build a box** and report what worked and what didn't (parts, print settings, NFC range, sound).
- **Improve the card spec** (`docs/card-spec.md`). Discuss larger changes in an issue first: other boxes depend on the format.
- **Write code.** See the roadmap in the README for what's next.
- **Design enclosures.** Put new designs under `hardware/` with their own README.

## Development setup

You need [rustup](https://rustup.rs). The pinned toolchain in `rust-toolchain.toml` installs itself on first use.

```sh
git clone https://github.com/MathiasNI/tapbox
cd tapbox
cargo test --workspace
```

No hardware is needed: `tapbox_hw::MemoryTag` simulates an NTAG215, and the `tapbox` CLI encodes and decodes cards.

## Before opening a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same checks, plus a `no_std` build and a Raspberry Pi (aarch64) build.

## Code style

- Readable over clever. Small functions, descriptive names, doc comments on every public item.
- Strong types at API boundaries (`WebUrl`, `ContentHash`, `Card`), not raw strings.
- No `unsafe`, and no `unwrap`/`expect`/`panic` outside tests: return errors.
- `tapbox-card` and `tapbox-hw` stay `no_std` so they can run on microcontrollers and in the browser.
- Card data is untrusted. Validate it, bound its size, and never accept local paths.

## Changing the card format

The format is shared between boxes, so changes go in this order:

1. Update `docs/card-spec.md`.
2. Update `card-spec/tools/generate_vectors.py` and run it.
3. Update the Rust code until `cargo test` passes against the new vectors.

New optional keys are fine in a minor revision. Anything that breaks old cards needs a new major version (`v`).

## Safety

This is a toy for children. Changes touching batteries, charging, volume limits or mechanical parts get extra review. When in doubt, choose the safer option and say why in the pull request.
