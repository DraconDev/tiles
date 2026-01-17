{
  description = "Tiles - High-performance modular data commander built on Terma";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustVersion = pkgs.rust-bin.stable.latest.default;

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustVersion;
          rustc = rustVersion;
        };

        tiles = rustPlatform.buildRustPackage {
          pname = "tiles";
          version = "0.1.3579";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "terma-1.0.1606" = "sha256-Xbq3IzBqbYLaZZpu//rddI7M1sBgjv46mT86Ots8qnA=";
            };
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            perl
          ];

          buildInputs = with pkgs; [
            openssl
          ];

          # Disable check because tests might require TTY/network
          doCheck = false;

          meta = with nixpkgs.lib; {
            description = "High-performance modular data commander";
            homepage = "https://github.com/DraconDev/tiles";
            license = licenses.mit;
            maintainers = [ ];
          };
        };
      in
      {
        packages.default = tiles;

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustVersion
            pkgs.pkg-config
            pkgs.openssl
          ];
        };
      }
    );
}
