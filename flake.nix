{
  description = "language-engine-witness — process-level acceptance witness for the minimal language engine: drives the Ethos, Nomos, and Logos daemons over live signal contracts and asserts the emitted Rust compiles and behaves (working programs), with durable restart recovery";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-build = {
      url = "github:LiGoldragon/rust-build";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    sema-storage.url = "github:LiGoldragon/sema-storage/269953164460cd842c1f3f8c9c93e4afe1e3628e";
    ethos-engine.url = "github:LiGoldragon/ethos-engine/ed12804a35377a8c6dcdd7325c126b394e7dff02";
    nomos-engine.url = "github:LiGoldragon/nomos-engine/c679660b50e5ef1be4871e586feb6ebc075f81f6";
    logos-engine.url = "github:LiGoldragon/logos-engine/7f75d37513b967b9b8581aa707f513379bb74bac";
    signal-domain-source = {
      url = "github:LiGoldragon/signal-domain/c24059de43614e6fb2128e47f959dc11748bd7e7";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-build, sema-storage, ethos-engine, nomos-engine, logos-engine, signal-domain-source }:
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
            sema-storage.packages.${system}.default
            ethos-engine.packages.${system}.default
            nomos-engine.packages.${system}.default
            logos-engine.packages.${system}.default
          ];
          SEMA_STORAGE_BIN = "${sema-storage.packages.${system}.default}/bin/sema-storage";
          ETHOS_ENGINE_BIN = "${ethos-engine.packages.${system}.default}/bin/ethos-engine";
          NOMOS_ENGINE_BIN = "${nomos-engine.packages.${system}.default}/bin/nomos-engine";
          LOGOS_ENGINE_BIN = "${logos-engine.packages.${system}.default}/bin/logos-engine";
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
