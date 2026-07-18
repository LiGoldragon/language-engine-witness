{
  description = "language-engine-witness — process-level acceptance witness for the minimal language engine: drives the schema, nomos, and logos daemons over live signal contracts and asserts the emitted Rust compiles and behaves (working programs), with durable restart recovery";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-build = {
      url = "github:LiGoldragon/rust-build";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    sema-storage.url = "github:LiGoldragon/sema-storage/269953164460cd842c1f3f8c9c93e4afe1e3628e";
    schema-engine.url = "github:LiGoldragon/schema-engine/9ba190dca112ca70c6383514793cc12e7913d488";
    nomos-engine.url = "github:LiGoldragon/nomos-engine/526baea6af52a3c39b9fd84a42658014f3423bb4";
    logos-engine.url = "github:LiGoldragon/logos-engine/7f75d37513b967b9b8581aa707f513379bb74bac";
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
