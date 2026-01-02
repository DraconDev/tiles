{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  name = "tiles-dev";
  
  buildInputs = with pkgs; [
    # Rust
    rustup
    pkg-config
    openssl
    
    # Windowing (runtime deps for winit/softbuffer)
    wayland
    wayland-protocols
    libxkbcommon
    xorg.libX11
    xorg.libXcursor
    xorg.libXrandr
    xorg.libXi
    xorg.libxcb
  ];
  
  # Set LD_LIBRARY_PATH so winit can dlopen wayland libs
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
    pkgs.wayland
    pkgs.libxkbcommon
  ];
  
  shellHook = ''
    echo "🖼️ Tiles dev shell ready!"
    echo "Run: cargo run --release -- --window"
  '';
}
