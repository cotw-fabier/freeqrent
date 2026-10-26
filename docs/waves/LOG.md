# Wave log

One short entry per W wave, newest first. The protocol is in `README.md`.

## 20261025204222 · M0 · Workspace and encoder core

- Landed: Cargo workspace; `freeqrent` core (`no_std` + `alloc`, zero deps). Encoder for v1–40 × L/M/Q/H with numeric/alphanumeric/byte/ECI segments, RS + interleaving, function patterns, zigzag placement, 8 masks with ISO penalty scoring, `QrMatrix` with per-module roles, builder (version range/pin, ECC boost, forced mask, ECI policy), `capacity_report()`.
- Decisions: `ModuleRole` gains `DarkModule` and `Remainder`; ECI 26 only for non-ASCII text by default; `boost_ecc` on by default; feature flags beyond `std` get declared only when they do something.
- Checks: 24 unit + 7 round-trip + 2 doc tests. `rqrr` decodes every version × ECC at full capacity, every forced mask, mixed segments, UTF-8 with and without ECI, and pixel renders v1–40. Release: v10 encode ≈0.24 ms, v40 ≈2.5 ms.
- Next (M0): minimal mixed-mode segmentation, Kanji feature, known-answer vectors, `cargo fuzz`, `cargo deny`.

## 20261025203745 · pre-M0 · Spec and project setup

- Landed: `SPEC.md` draft v0.3 (core features, animation presets, art mode, bindings design), MIT `LICENSE`, wave protocol, `ROADMAP.md`, `AGENTS.md`.
- Decisions: name freeQRent (`freeqrent` everywhere); Rust core first (Phase 1), bindings second (Phase 2); SVG and animated SVG output only; features 4 and 7 interpreted as styled finder/alignment/timing patterns and art mode (§7.2, §7.8).
- Next: M0, the in-house QR encoder.
