{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/release-24.11";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs = inputs@{ self, nixpkgs, flake-parts, rust-overlay }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" ];

      perSystem = { system, ... }:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };
        in {
          _module.args.pkgs = pkgs;

          devShells.default = with pkgs;
            mkShell rec {
              packages = [
                (rust-bin.nightly."2025-01-28".default.override {
                  extensions = [ "rust-src" "rust-analyzer" ];
                  targets = [ "wasm32-unknown-unknown" ];
                })
              ];
              RUST_BACKTRACE = 1;
              LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath packages}";
            };
        };
    };
}
