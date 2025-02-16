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
  };

  outputs = {
    nixpkgs,
    crane,
    fenix,
    ...
  }: let
    forAllSystems = function:
      nixpkgs.lib.genAttrs [
        "x86_64-linux"
      ] (system:
        function (import nixpkgs {
          inherit system;
          overlays = [fenix.overlays.default];
        }));
  in {
    formatter = forAllSystems (pkgs: pkgs.alejandra);

    packages = forAllSystems (pkgs: let
      craneLib = crane.mkLib pkgs;

      commonArgs = {
        src = craneLib.cleanCargoSource ./.;
        strictDeps = true;
      };
    in {
      default = craneLib.buildPackage (commonArgs
        // {
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        });
    });

    devShells = forAllSystems (pkgs: {
      default = pkgs.mkShell {
        nativeBuildInputs = [
          pkgs.rust-analyzer-nightly
          pkgs.fenix.stable.defaultToolchain
        ];
      };
    });
  };
}
