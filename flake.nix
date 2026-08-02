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
      url = "github:LiGoldragon/nomos-engine/f95b38c6805a031fbf7adad78234349d784d9845";
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
          NOMOS_GENERATOR_BIN = "${nomos-engine.packages.${system}.default}/bin/nomos-generate";
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
          offline-generator = pkgs.runCommand "offline-generator" {
            nativeBuildInputs = [ nomos-engine.packages.${system}.default ];
          } ''
            mkdir -p $out
            nomos-generate ${./tests/fixtures/batch-config.json} ${./tests/fixtures/interface.ethos} $out/interface.rs $out/interface.outcome
            nomos-generate ${./tests/fixtures/batch-config.json} ${./tests/fixtures/nexus.ethos} $out/nexus.rs $out/nexus.outcome
            nomos-generate ${./tests/fixtures/batch-config.json} ${./tests/fixtures/sema.ethos} $out/sema.rs $out/sema.outcome
            test -s $out/interface.rs
            test -s $out/nexus.rs
            test -s $out/sema.rs
            grep -q '^deferred 10$' $out/interface.outcome
            grep -q '^deferred 0$' $out/nexus.outcome
            grep -q '^deferred 3$' $out/sema.outcome
          '';
        };
        devShells.default = pkgs.mkShell {
          name = "language-engine-witness";
          packages = [ pkgs.jujutsu toolchain ];
        };
      });
}
