# freeqrent_example

`FreeQrentAnimated` in an app: an intro played on build (with a replay button and a palette switch that tweens in OKLab), and codes below that play as they scroll into view.

```sh
flutter run            # macOS, iOS or Android
flutter build apk      # the hook builds libfreeqrent_dart.so for each Android ABI
```

The first build compiles the Rust core through `freeqrent`'s Native Assets hook, so it needs rustup (the toolchain in `freeqrent/rust/rust-toolchain.toml` is installed on demand).
