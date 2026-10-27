//! The Dart package's native library: freeqrent-ffi's C ABI
//! (`crates/freeqrent-ffi/include/freeqrent.h`), re-exported so the
//! library this crate builds carries every `freeqrent_*` symbol. The Dart
//! side binds them with `@Native` (`lib/src/ffi.dart`).
//!
//! `pub use` keeps the dependency linked; the functions themselves are
//! `#[unsafe(no_mangle)] extern "C"` in freeqrent-ffi, so a cdylib or a
//! staticlib exports them as they are.

#![forbid(unsafe_code)]

pub use freeqrent_ffi::*;
