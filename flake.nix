{
  description = "language-engine-witness — process-level acceptance witness for the minimal language engine: drives the schema, nomos, and logos daemons over live signal contracts and asserts the byte-exact generated-Rust closure";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-build = {
      url = "github:LiGoldragon/rust-build";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    sema-storage.url = "github:LiGoldragon/sema-storage/a088dc14bcc6b94ab1b0c3e41e2250ddf1e7b06f";
    schema-engine.url = "github:LiGoldragon/schema-engine/bf05d1059fb017e04596ee711326ef2b61f239ac";
    nomos-engine.url = "github:LiGoldragon/nomos-engine/f129677fcc05cbce48f0c32eb2ecd0915c9940a4";
    logos-engine.url = "github:LiGoldragon/logos-engine/d67d2e88a812e8045331d8237c714bea7eb68aad";
  };

  outputs = { self, nixpkgs, flake-utils, rust-build, sema-storage, schema-engine, nomos-engine, logos-engine }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        rust = rust-build.lib.${system}.fromPkgs pkgs;
        inherit (rust) craneLib toolchain;
        # The acceptance fixture is a .schema file; preserve non-Rust test data.
        src = pkgs.lib.cleanSource ./.;
        commonArguments = { inherit src; strictDeps = true; };
        cargoArtifacts = craneLib.buildDepsOnly commonArguments;
      in
      {
        packages.default = craneLib.buildPackage (commonArguments // { inherit cargoArtifacts; });
        checks = {
          build = craneLib.cargoBuild (commonArguments // { inherit cargoArtifacts; });
          test = craneLib.cargoTest (commonArguments // {
            inherit cargoArtifacts;
            nativeBuildInputs = [
              sema-storage.packages.${system}.default
              schema-engine.packages.${system}.default
              nomos-engine.packages.${system}.default
              logos-engine.packages.${system}.default
            ];
            SEMA_STORAGE_BIN = "${sema-storage.packages.${system}.default}/bin/sema-storage";
            SCHEMA_ENGINE_BIN = "${schema-engine.packages.${system}.default}/bin/schema-engine";
            NOMOS_ENGINE_BIN = "${nomos-engine.packages.${system}.default}/bin/nomos-engine";
            LOGOS_ENGINE_BIN = "${logos-engine.packages.${system}.default}/bin/logos-engine";
          });
          doc = craneLib.cargoDoc (commonArguments // {
            inherit cargoArtifacts;
            RUSTDOCFLAGS = "-D warnings";
          });
          fmt = craneLib.cargoFmt { inherit src; };
          clippy = craneLib.cargoClippy (commonArguments // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- -D warnings";
          });
        };
        devShells.default = pkgs.mkShell {
          name = "language-engine-witness";
          packages = [ pkgs.jujutsu toolchain ];
        };
      });
}
