{
  description = "mandelbrot — render the Mandelbrot set as terminal art or a PNG";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: nixpkgs.legacyPackages.${system};
    in
    {
      packages = forAllSystems (system: {
        default = (pkgsFor system).callPackage ./mandelbrot.nix { };
      });

      # `nix build` runs the unit tests: mandelbrot.nix sets doCheck, so the
      # test phase is part of building the package and needs no separate target.
      # `nix flake check` additionally builds every system and runs the checks
      # that come with the outputs above.

      # Same pinned toolchain as the package build, so `cargo test` in here
      # agrees with the package's own test phase.
      devShells = forAllSystems (system: {
        default =
          let
            pkgs = pkgsFor system;
          in
          pkgs.mkShell {
            packages = [
              pkgs.cargo
              pkgs.rustc
              pkgs.rustfmt
              pkgs.clippy
            ];
          };
      });

      formatter = forAllSystems (system: (pkgsFor system).nixfmt);
    };
}
