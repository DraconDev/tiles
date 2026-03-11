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
          version = "0.1.14367";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            perl
            makeWrapper
          ];

          buildInputs = with pkgs; [
            openssl
          ];

          postInstall = ''
            wrapProgram "$out/bin/tiles" \
              --prefix PATH : ${
                pkgs.lib.makeBinPath [
                  pkgs.wl-clipboard
                  pkgs.xclip
                  pkgs.xsel
                ]
              }
          '';

          # Disable check because tests might require TTY/network
          doCheck = false;

          meta = with nixpkgs.lib; {
            description = "High-performance modular data commander";
            homepage = "https://github.com/DraconDev/tiles";
            license = licenses.unfree;
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
            pkgs.wl-clipboard
            pkgs.xclip
            pkgs.xsel
          ];
        };
      }
    );
}
