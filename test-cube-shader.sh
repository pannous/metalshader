#!/bin/bash
# Test the metalshader-compatible cube shader

set -e

PORT=2222
SHADER_DIR="/root/shaders"

echo "Copying cube shaders to guest..."
scp -P "$PORT" -o StrictHostKeyChecking=no \
    shaders/cube.vert.spv \
    shaders/cube.frag.spv \
    shaders/cube.frag \
    root@localhost:"$SHADER_DIR/"

echo ""
echo "Testing cube shader with metalshader..."
echo "Controls: Arrow keys to navigate, ESC/Q to quit"
echo ""

ssh -p "$PORT" -o StrictHostKeyChecking=no root@localhost \
    "cd /root && ./metalshader cube"
