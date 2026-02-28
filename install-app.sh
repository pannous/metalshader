#!/bin/bash
# Build and install MetalShader.app to /Applications
set -e

APP=/Applications/MetalShader.app
BINARY=$APP/Contents/MacOS/metalshader
SHADERS=$APP/Contents/Resources/shaders

echo "Building metalshader..."
cargo build --release

echo "Installing to $APP..."
cp target/release/metalshader "$BINARY"
rsync -a --delete shaders/ "$SHADERS/"

echo "Ad-hoc signing..."
codesign --force --deep --sign - "$APP"

echo "Re-registering with Launch Services..."
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP"

echo "✓ Installed $(ls $SHADERS/*.spv | wc -l | tr -d ' ') shaders"
echo "✓ MetalShader.app ready — you can now set it as default in Finder's 'Get Info'"
