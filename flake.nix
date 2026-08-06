{
  description = "language-engine-witness — acceptance for authority-sealed bootstrap schemas and current process boundaries";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-build = {
      url = "github:LiGoldragon/rust-build";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    signal-domain-source = {
      url = "github:LiGoldragon/signal-domain/39d2313c821b0ed7a8b8b646bb081c409d10dcb4";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-build, signal-domain-source }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        rust = rust-build.lib.${system}.fromPkgs pkgs;
        inherit (rust) craneLib toolchain;
        # The acceptance fixture is an .ethos file; preserve non-Rust test data.
        src = pkgs.lib.cleanSource ./.;
        commonArguments = {
          inherit src;
          strictDeps = true;
        };
        cargoArtifacts = craneLib.buildDepsOnly commonArguments;
      in
      {
        packages.default = craneLib.buildPackage (commonArguments // { inherit cargoArtifacts; });
        checks = {
          build = craneLib.cargoBuild (commonArguments // { inherit cargoArtifacts; });
          test = craneLib.cargoTest (commonArguments // { inherit cargoArtifacts; });
          doc = craneLib.cargoDoc (commonArguments // {
            inherit cargoArtifacts;
            RUSTDOCFLAGS = "-D warnings";
          });
          fmt = craneLib.cargoFmt { inherit src; };
          clippy = craneLib.cargoClippy (commonArguments // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- -D warnings";
          });
          spirit-domain-inventory = pkgs.runCommand "spirit-domain-inventory" {} ''
            cmp ${signal-domain-source}/schema/domain.schema ${./tests/fixtures/spirit-domain.ethos}
            touch $out
          '';
        };
        devShells.default = pkgs.mkShell {
          name = "language-engine-witness";
          packages = [ pkgs.jujutsu toolchain ];
        };
      });
}
