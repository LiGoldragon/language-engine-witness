{
  description = "language-engine-witness — process-level acceptance witness for authority-authored, stateful native Nomos deployment and restart recovery";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-build = {
      url = "github:LiGoldragon/rust-build";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nomos-engine = {
      url = "github:LiGoldragon/nomos-engine/e4230f62b55fcf8543477a26d272862a63aa1fc3";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
      inputs.rust-build.follows = "rust-build";
    };
    sema-translator = {
      url = "github:LiGoldragon/sema-translator/6df830ab1ec9f315a5b50e40ffc393b48ea3d412";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-build.follows = "rust-build";
    };
    signal-domain-source = {
      url = "github:LiGoldragon/signal-domain/c24059de43614e6fb2128e47f959dc11748bd7e7";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-build, nomos-engine, sema-translator, signal-domain-source }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        rust = rust-build.lib.${system}.fromPkgs pkgs;
        inherit (rust) craneLib toolchain;
        # The acceptance fixture is an .ethos file; preserve non-Rust test data.
        src = pkgs.lib.cleanSource ./.;
        commonArguments = { inherit src; strictDeps = true; };
        cargoArtifacts = craneLib.buildDepsOnly commonArguments;
        processTestArguments = {
          nativeBuildInputs = [
            nomos-engine.packages.${system}.default
            sema-translator.packages.${system}.default
          ];
          NOMOS_ENGINE_BIN = "${nomos-engine.packages.${system}.default}/bin/nomos-engine";
          SEMA_TRANSLATOR_BIN = "${sema-translator.packages.${system}.default}/bin/sema-translator-daemon";
        };
      in
      {
        packages.default = craneLib.buildPackage (commonArguments // { inherit cargoArtifacts; } // processTestArguments);
        checks = {
          build = craneLib.cargoBuild (commonArguments // { inherit cargoArtifacts; });
          test = craneLib.cargoTest (commonArguments // { inherit cargoArtifacts; } // processTestArguments);
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
