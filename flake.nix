{
  description = "core-nomos — the stringless Core of Nomos: macros as typed data lowering CoreSchema to CoreLogos, with the real generated Rust as the acceptance oracle";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-build = {
      url = "github:LiGoldragon/rust-build";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    sema-storage.url = "github:LiGoldragon/sema-storage/929b9f696e2c23dd0e338546140c52fe9f046163";
    schema-engine.url = "github:LiGoldragon/schema-engine/7b61b70100bbaf12f32d930a3decfb6d5b534ad8";
    nomos-engine.url = "github:LiGoldragon/nomos-engine/458104b89a407b30715f264f023fc045109443cd";
    logos-engine.url = "github:LiGoldragon/logos-engine/43d8f279938e5c52ca1032a3467947095695cd1d";
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
          name = "core-nomos";
          packages = [ pkgs.jujutsu toolchain ];
        };
      });
}
