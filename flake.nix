{
  description = "language-engine-witness — process-level acceptance witness for the minimal language engine: drives the schema, nomos, and logos daemons over live signal contracts and asserts the byte-exact generated-Rust closure";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-build = {
      url = "github:LiGoldragon/rust-build";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    sema-storage.url = "github:LiGoldragon/sema-storage/666a06caee2c16fe7590e10ef9fa22bc6277a060";
    schema-engine.url = "github:LiGoldragon/schema-engine/78f6a37e013d983e5ae5458b31c1760efbcbf198";
    nomos-engine.url = "github:LiGoldragon/nomos-engine/6d8cc09e0a65a746799ff32fd5a97c412232cb46";
    logos-engine.url = "github:LiGoldragon/logos-engine/d427fe1ce279e931d9a44b617f24c7701493102f";
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
