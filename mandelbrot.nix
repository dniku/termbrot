# The package, kept out of flake.nix so `nix flake show` stays cheap.
{
  lib,
  rustPlatform,
}:
rustPlatform.buildRustPackage {
  pname = "mandelbrot";
  version = "0.1.0";

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./src
      ./Cargo.toml
    ];
  };

  # The crate depends only on the standard library, so there is no crates.io
  # archive to fetch and no `cargoHash` to keep in sync. buildRustPackage still
  # insists on one of cargoHash/cargoVendorDir/cargoDeps/cargoLock, and reads
  # cargoVendorDir as a path *relative to the source root* -- hence the checked-in
  # empty `cargo-vendor/` directory rather than a synthesized store path. That
  # directory states "nothing to vendor" without inventing a hash whose meaning
  # nobody could check.
  cargoVendorDir = "cargo-vendor";

  doCheck = true;

  meta = {
    description = "Render the Mandelbrot set to blocky terminal art or a PNG";
    license = lib.licenses.mit;
    mainProgram = "mandelbrot";
    platforms = lib.platforms.all;
  };
}
