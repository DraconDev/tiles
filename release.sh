#!/bin/bash
set -e

VERSION=$(grep '^version =' Cargo.toml | head -n1 | cut -d '"' -f2)
echo "Building Tiles v$VERSION..."

cargo build --release

mkdir -p releases
cp target/release/tiles "releases/tiles-v$VERSION-linux"

echo "Build complete: releases/tiles-v$VERSION-linux"
echo "To release on GitHub:"
echo "1. git tag v$VERSION"
echo "2. git push origin v$VERSION"
