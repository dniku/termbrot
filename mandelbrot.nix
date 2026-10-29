# The package, kept out of flake.nix so `nix flake show` stays cheap.
#
# No `cargoHash`: the crate has no dependencies outside the standard library,
# so there is nothing to fetch and no vendor hash to keep in sync. Adding one
# would imply a lockfile that does not exist.
{
  lib,
  rustPlatform,
  ...
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

  doCheck = true;

  meta = {
    description = "Render the Mandelbrot set to blocky terminal art or a PNG";
    homepage = "https://github.com/example/mandelbrot";
    license = lib.licenses.mit;
    mainProgram = "mandelbrot";
    platforms = lib.platforms.all;
  };
}
