#!/bin/sh
# Build metalshader on Alpine Linux guest

echo "Building metalshader..."
gcc -I/usr/include/libdrm -o metalshader metalshader.c -ldrm -lvulkan -lgbm -lm

if [ $? -eq 0 ]; then
    echo "✓ Build successful: ./metalshader"
    ls -lh metalshader
else
    echo "✗ Build failed"
    exit 1
fi
