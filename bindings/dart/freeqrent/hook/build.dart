// The Native Assets build hook: Dart and Flutter run it before a build or
// a test, and it compiles rust/ (freeqrent-ffi's C ABI) for the target
// being built. native_toolchain_rust drives rustup with the toolchain in
// rust/rust-toolchain.toml, so the first build installs it if needed.
import 'package:hooks/hooks.dart';
import 'package:native_toolchain_rust/native_toolchain_rust.dart';

void main(List<String> args) async {
  await build(args, (input, output) async {
    await const RustBuilder(
      // Matches the asset id the @Native bindings use (lib/src/ffi.dart).
      assetName: 'src/ffi.dart',
    ).run(input: input, output: output);
  });
}
