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
      url = "github:LiGoldragon/nomos-engine/c20a1a2af92d344ed2568a0d40b91c13ab6b51a3";
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
    spirit-ethos = {
      url = "github:LiGoldragon/spirit-ethos/5bafccefe32c4b6d9b4587b97806384a57e848b7";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-build, nomos-engine, sema-translator, signal-domain-source, spirit-ethos }:
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
          SPIRIT_ETHOS_SOURCE = spirit-ethos;
        };
        cargoArtifacts = craneLib.buildDepsOnly commonArguments;
        processTestArguments = {
          nativeBuildInputs = [
            nomos-engine.packages.${system}.default
            sema-translator.packages.${system}.default
          ];
          NOMOS_ENGINE_BIN = "${nomos-engine.packages.${system}.default}/bin/nomos-engine";
          NOMOS_GENERATOR_BIN = "${nomos-engine.packages.${system}.default}/bin/nomos-generate";
          SEMA_TRANSLATOR_BIN = "${sema-translator.packages.${system}.default}/bin/sema-translator-daemon";
          # The isolated pinned-current-v14 fixture is a separate locked Cargo
          # graph whose Git sources cannot be fetched inside a Nix sandbox.
          # The ordinary, unsandboxed strict test executes that reader.
          SPIRIT_V14_READER_CARGO_UNAVAILABLE = "nix-network-sandbox";
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
          sealed-spirit-source = pkgs.runCommand "sealed-spirit-source" { } ''
            test -f ${spirit-ethos}/allocation-manifest.nota
            test -f ${spirit-ethos}/allocation-receipt.nota
            test -f ${spirit-ethos}/batch-config.json
            grep -Fx 'universal Entry 29' ${spirit-ethos}/allocation-manifest.nota
            grep -Fx 'database-marker commit-sequence=2 snapshot=2' ${spirit-ethos}/allocation-receipt.nota
            touch $out
          '';
        };
        devShells.default = pkgs.mkShell {
          name = "language-engine-witness";
          packages = [ pkgs.jujutsu toolchain ];
        };
      });
}
