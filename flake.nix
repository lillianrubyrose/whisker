{
  description = "RISC-V emulator made with love <3";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane = {
      url = "github:ipetkov/crane";
    };
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs = inputs @ {
    self,
    nixpkgs,
    crane,
    fenix,
    flake-parts,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux"];

      perSystem = {
        pkgs,
        system,
        ...
      }: let
        overlays = [fenix.overlays.default];
        pkgs = import nixpkgs {inherit system overlays;};
        pkgsRiscv = (import nixpkgs {inherit system;}).pkgsCross.riscv64;
        craneLib = crane.mkLib pkgs;
        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
        };
      in {
        formatter = pkgs.alejandra;

        packages.default = craneLib.buildPackage (commonArgs
          // {
            cargoArtifacts = craneLib.buildDepsOnly commonArgs;
          });

        devShells = {
          default = pkgs.mkShell {
            nativeBuildInputs = [
              pkgs.rust-analyzer-nightly
              pkgs.fenix.stable.defaultToolchain

              pkgsRiscv.buildPackages.gcc
              pkgsRiscv.buildPackages.binutils
            ];
          };
        };
      };
    };
}
