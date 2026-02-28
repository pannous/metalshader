#!/bin/sh
# Build metalshader (Rust version)

echo "Building metalshader (Rust)..."

# Check if we're cross-compiling or building natively
if [ "$1" = "cross" ]; then
    echo "Cross-compiling for aarch64-unknown-linux-musl (Alpine guest)..."
    cargo build --release --target aarch64-unknown-linux-musl
    if [ $? -eq 0 ]; then
        cp target/aarch64-unknown-linux-musl/release/metalshader ./metalshader-rust
        echo "✓ Cross-compile successful: ./metalshader-rust"
        ls -lh metalshader-rust
    else
        echo "✗ Cross-compile failed"
        exit 1
    fi
else
    echo "Building natively..."
    cargo build --release
    if [ $? -eq 0 ]; then
        cp target/release/metalshader ./metalshader
        echo "✓ Build successful: ./metalshader"
        ls -lh metalshader-rust
    else
        echo "✗ Build failed"
        exit 1
    fi
fi
