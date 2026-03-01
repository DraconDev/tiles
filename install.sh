#!/usr/bin/env bash
# Tiles Installer Script
set -e

# Configuration
BIN_NAME="tiles"
BIN_DEST="$HOME/.local/bin"
APP_DEST="$HOME/.local/share/applications"
ICON_DEST="$HOME/.local/share/icons"

echo "🚀 Installing $BIN_NAME..."

# 1. Build release
cargo build --release

# 2. Create directories
mkdir -p "$BIN_DEST"
mkdir -p "$APP_DEST"
mkdir -p "$ICON_DEST"

# 3. Copy binary
rm -f "$BIN_DEST/$BIN_NAME"
cp "target/release/$BIN_NAME" "$BIN_DEST/"
chmod +x "$BIN_DEST/$BIN_NAME"

# 4. Copy Icon
cp "tiles_icon.svg" "$ICON_DEST/tiles.svg"

# 5. Copy desktop entry
# Replace Exec and Icon with absolute paths
sed -e "s|Exec=tiles|Exec=$BIN_DEST/$BIN_NAME|" \
    -e "s|Icon=system-file-manager|Icon=$ICON_DEST/tiles.svg|" \
    tiles.desktop > "$APP_DEST/tiles.desktop"

# 6. Update desktop database
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database "$APP_DEST"
fi


echo "✅ Installation complete!"
echo "📍 Binary: $BIN_DEST/$BIN_NAME"
echo "📍 Desktop Entry: $APP_DEST/tiles.desktop"
echo ""

# Path verification
if [[ ":$PATH:" != *":$BIN_DEST:"* ]]; then
    echo "⚠️  Note: $BIN_DEST is not in your PATH."
    echo "You might want to add this to your .bashrc or .zshrc:"
    echo "export PATH=\"\$HOME/.local/bin:\$PATH\""
fi

echo "🎉 You can now launch 'Tiles' from your application menu!"
