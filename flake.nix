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

      # `nix test` runs the unit tests inside the Nix sandbox, so linking and
      # execution never depend on the host toolchain.
      tests = forAllSystems (system: {
        unit = self.packages.${system}.default.cargoTest;
      });

      # Same pinned toolchain as the package build, so `cargo test` in here
      # agrees with `nix test`.
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
              pkgs.cargo-clippy
            ];
          };
      });

      formatter = forAllSystems (system: (pkgsFor system).nixfmt-rfc-style);
    };
}
