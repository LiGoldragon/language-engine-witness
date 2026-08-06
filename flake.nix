{
  description = "language-engine-witness — acceptance for authority-sealed bootstrap schemas and current process boundaries";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-build = {
      url = "github:LiGoldragon/rust-build";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nomos-engine = {
      url = "github:LiGoldragon/nomos-engine/2ccb200894056abbaae70b10a070c427fa4fdf4c";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
      inputs.rust-build.follows = "rust-build";
    };
    sema-translator = {
      url = "github:LiGoldragon/sema-translator/4675e5ddfdd0d24144498ec9b7d2e5b9cb422249";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-build.follows = "rust-build";
    };
    signal-domain-source = {
      url = "github:LiGoldragon/signal-domain/6f7c1352602581cb6cb82f507fe573890c6ffa56";
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
        commonArguments = {
          inherit src;
          strictDeps = true;
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
