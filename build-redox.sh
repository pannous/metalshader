#!/bin/bash
# Build metalshader for Redox OS

set -e

echo "Building metalshader for Redox OS (aarch64)..."

# Ensure Redox target is installed
if ! rustup target list --installed | grep -q "aarch64-unknown-redox"; then
    echo "Installing aarch64-unknown-redox target..."
    rustup target add aarch64-unknown-redox
fi

# Build for Redox
echo "Running cargo build..."
cargo build --target aarch64-unknown-redox --release

# Show the binary location
BINARY="target/aarch64-unknown-redox/release/metalshader"
if [ -f "$BINARY" ]; then
    echo "✓ Build successful!"
    echo "  Binary: $BINARY"
    echo "  Size: $(du -h "$BINARY" | cut -f1)"
    file "$BINARY"
else
    echo "✗ Build failed - binary not found"
    exit 1
fi
