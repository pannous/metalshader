#!/bin/sh
# Install Rust metalshader to Alpine guest

GUEST_DIR="/root/"

echo "Installing Rust metalshader to guest..."

# Copy binary
scp -P 2222 -o StrictHostKeyChecking=no \
    metalshader-rust root@localhost:${GUEST_DIR}/metalshader-rust

# Copy shaders directory if needed
if [ "$1" = "with-shaders" ]; then
    echo "Copying shaders..."
    scp -P 2222 -o StrictHostKeyChecking=no -r \
        shaders root@localhost:${GUEST_DIR}/
fi

echo "✓ Installed to guest:${GUEST_DIR}/metalshader-rust"
echo ""
echo "To run on guest:"
echo "  ssh -p 2222 root@localhost"
echo "  cd ${GUEST_DIR}"
echo "  ./metalshader-rust <shader_name>"
