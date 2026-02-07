#!/bin/bash
# Check if Vulkan/MoltenVK is properly installed on macOS

echo "=== Vulkan/MoltenVK Installation Check ==="
echo ""

# Check for Vulkan SDK
if [ -n "$VULKAN_SDK" ]; then
    echo "✅ VULKAN_SDK is set: $VULKAN_SDK"

    if [ -f "$VULKAN_SDK/lib/libvulkan.1.dylib" ]; then
        echo "✅ libvulkan found in SDK"
    else
        echo "❌ libvulkan NOT found in SDK"
    fi
else
    echo "❌ VULKAN_SDK environment variable not set"
fi

echo ""

# Check for MoltenVK via Homebrew
if [ -f "/opt/homebrew/lib/libMoltenVK.dylib" ]; then
    echo "✅ MoltenVK found via Homebrew (Apple Silicon)"
elif [ -f "/usr/local/lib/libMoltenVK.dylib" ]; then
    echo "✅ MoltenVK found via Homebrew (Intel)"
else
    echo "❌ MoltenVK not found via Homebrew"
fi

echo ""

# Check DYLD_LIBRARY_PATH
if [ -n "$DYLD_LIBRARY_PATH" ]; then
    echo "✅ DYLD_LIBRARY_PATH is set: $DYLD_LIBRARY_PATH"
else
    echo "⚠️  DYLD_LIBRARY_PATH not set (may be optional)"
fi

echo ""

# Try to find libvulkan
echo "Searching for libvulkan..."
VULKAN_LIBS=$(find /opt/homebrew /usr/local /Library ~/VulkanSDK -name "libvulkan*.dylib" 2>/dev/null)

if [ -n "$VULKAN_LIBS" ]; then
    echo "✅ Found Vulkan libraries:"
    echo "$VULKAN_LIBS" | head -5
else
    echo "❌ No Vulkan libraries found"
fi

echo ""
echo "=== Setup Instructions ==="
echo ""

if [ -z "$VULKAN_SDK" ] && [ ! -f "/opt/homebrew/lib/libMoltenVK.dylib" ] && [ ! -f "/usr/local/lib/libMoltenVK.dylib" ]; then
    echo "You need to install Vulkan SDK or MoltenVK:"
    echo ""
    echo "Option 1: Vulkan SDK (Recommended)"
    echo "  1. Download from: https://vulkan.lunarg.com/sdk/home"
    echo "  2. Run the installer"
    echo "  3. Add to ~/.zshrc or ~/.bash_profile:"
    echo "     export VULKAN_SDK=\"\$HOME/VulkanSDK/<version>/macOS\""
    echo "     export PATH=\"\$VULKAN_SDK/bin:\$PATH\""
    echo "     export DYLD_LIBRARY_PATH=\"\$VULKAN_SDK/lib:\$DYLD_LIBRARY_PATH\""
    echo "     export VK_ICD_FILENAMES=\"\$VULKAN_SDK/share/vulkan/icd.d/MoltenVK_icd.json\""
    echo "  4. Reload: source ~/.zshrc"
    echo ""
    echo "Option 2: Homebrew MoltenVK"
    echo "  brew install molten-vk"
    echo "  export DYLD_LIBRARY_PATH=\"/opt/homebrew/lib:\$DYLD_LIBRARY_PATH\""
    echo ""
fi

echo "Then run: ./target/release/metalshader example"
