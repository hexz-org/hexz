{
  description = "Hexz — deduplicated large file storage";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" ];
        };
      in {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [
            rustToolchain
            pkgs.cargo-deny
            pkgs.cargo-nextest
            pkgs.cargo-llvm-cov
            pkgs.pkg-config
            pkgs.gnumake
            pkgs.python313
            pkgs.maturin
            pkgs.ruff
            pkgs.git
            pkgs.bash
            pkgs.minio-client
          ];

          buildInputs = [
            pkgs.fuse3
            pkgs.fuse3.dev
            pkgs.openssl
            pkgs.stdenv.cc.cc.lib
          ];

          shellHook = ''
            export SHELL=${pkgs.bash}/bin/bash
            export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib:$LD_LIBRARY_PATH"
            if [ ! -d .venv ]; then
              python3 -m venv .venv
            fi
          '';
        };
      });
}
