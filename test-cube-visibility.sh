#!/bin/bash
# Test cube visibility debugging

PORT=2222

echo "Testing cube visibility variations..."
echo ""

echo "1. Testing cube_bright (no lighting, guaranteed bright):"
ssh -p $PORT root@localhost "cd /root && timeout 2 ./metalshader cube_bright 2>&1 | grep -E 'shader|FPS' || true"

echo ""
echo "2. Testing cube_test (cyan background, magenta cube):"
ssh -p $PORT root@localhost "cd /root && timeout 2 ./metalshader cube_test 2>&1 | grep -E 'shader|FPS' || true"

echo ""
echo "3. Testing plasma (known working):"
ssh -p $PORT root@localhost "cd /root && timeout 2 ./metalshader plasma 2>&1 | grep -E 'shader|FPS' || true"

echo ""
echo "If you see colors for plasma but black for cube variants,"
echo "the issue is with the cube shader math or camera setup."
