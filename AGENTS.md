# freeQRent: working agreement

A zero-dependency Rust library for styled and animated SVG QR codes. `SPEC.md` is the source of truth for what we're building.

## Read first

1. `SPEC.md`
2. `docs/waves/README.md`: how work is organized (M waves made of timestamp-named W waves)
3. `docs/waves/LOG.md`: what has landed so far
4. `ROADMAP.md`: ideas that aren't in the spec yet
5. The current wave's plan and notes in `docs/waves/notes/` (gitignored; they exist locally only)

## Rules

- **Phase 1 is the Rust core only.** Don't build language bindings until Phase 2 (SPEC §2, §12). Do keep the core binding-friendly: options are plain data, there are no callbacks on hot paths, and errors are structured.
- **The core crate has zero dependencies** and is `no_std` + `alloc`, with `#![forbid(unsafe_code)]`. Dev-dependencies (`rqrr`, `insta`, `resvg`) are fine. Anything else needs a spec change first.
- **Output is deterministic.** The same input produces byte-identical SVG. All randomness comes from the seeded PRNG.
- **Scannability is a hard constraint.** A style that can't be verified to scan doesn't ship. Round-trip decode tests cover every new style option.
- **The static state is the finished code.** Animation only plays *into* it (SPEC §8.2).
- **The spec moves with the code.** If a wave changes behavior described in `SPEC.md`, update the spec in the same wave. Ideas that come up but aren't built go in `ROADMAP.md`.
- **Every W wave ends with a `LOG.md` entry.**

## Checks (all clean before a wave closes)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
cargo build -p freeqrent --no-default-features   # no_std build
scripts/check-core-deps.sh                        # core stays dependency-free
cargo deny check
```

CI (`.github/workflows/ci.yml`) also builds for a bare-metal target, checks the MSRV (1.92), and runs each fuzz target for 60 s. Fuzz locally with `cd fuzz && cargo +nightly fuzz run encode_text`.

Never weaken or delete a test to get green. Never present a TODO stub as done.
