#!/bin/sh
# Build and install metalshader to /root/
set -e
echo '=== Building metalshader ==='
cargo build --release
echo ''
echo '=== Installing to /root/ ==='
cp target/release/metalshader /root/
echo '✓ Binary installed at /root/metalshader'
ls -lh /root/metalshader
