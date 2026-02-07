#!/bin/bash
# Test fixed cube shaders

set -e

PORT=2222
SHADER_DIR="/root/shaders"

echo "=== Testing cube_simple (guaranteed safe) ==="
scp -P "$PORT" -o StrictHostKeyChecking=no \
    shaders/cube_simple.vert.spv \
    shaders/cube_simple.frag.spv \
    shaders/cube_simple.frag \
    root@localhost:"$SHADER_DIR/"

echo "Running cube_simple for 3 seconds..."
ssh -p "$PORT" -o StrictHostKeyChecking=no root@localhost \
    "cd /root && timeout 3 ./metalshader cube_simple || true"

echo ""
echo "=== Testing cube (fixed with safe division) ==="
scp -P "$PORT" -o StrictHostKeyChecking=no \
    shaders/cube.vert.spv \
    shaders/cube.frag.spv \
    shaders/cube.frag \
    root@localhost:"$SHADER_DIR/"

echo "Running cube for 3 seconds..."
ssh -p "$PORT" -o StrictHostKeyChecking=no root@localhost \
    "cd /root && timeout 3 ./metalshader cube || true"

echo ""
echo "Done! If both ran without fence errors, the fixes worked."
