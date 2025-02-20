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
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
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

      imports = [
        inputs.git-hooks.flakeModule
      ];

      perSystem = {
        pkgs,
        system,
        config,
        ...
      }: let
        overlays = [fenix.overlays.default];
        pkgs = import nixpkgs {inherit system overlays;};
        pkgsRiscv = (import nixpkgs {inherit system;}).pkgsCross.riscv64;
        pre-commit-hooks = inputs.pre-commit-hooks.lib.${system};
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

        pre-commit.settings.hooks = {
          clippy = {
            enable = true;
            packageOverrides = {
              cargo = pkgs.fenix.stable.cargo;
              clippy = pkgs.fenix.stable.clippy;
            };
          };
          rustfmt = {
            enable = true;
            packageOverrides = {
              cargo = pkgs.fenix.stable.cargo;
              rustfmt = pkgs.fenix.stable.rustfmt;
            };
          };
        };

        devShells = {
          default = pkgs.mkShell {
            nativeBuildInputs = [
              pkgs.rust-analyzer
              pkgs.fenix.stable.defaultToolchain
              pkgs.clang-tools

              pkgsRiscv.buildPackages.gcc
              pkgsRiscv.buildPackages.gdb
              pkgsRiscv.buildPackages.binutils
            ];

            shellHook = ''
              ${config.pre-commit.installationScript}
            '';
          };
        };
      };
    };
}
