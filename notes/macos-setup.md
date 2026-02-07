# macOS Setup Guide

## Installing Vulkan/MoltenVK

metalshader requires Vulkan support on macOS, which is provided by MoltenVK (translates Vulkan to Metal).

### Option 1: Vulkan SDK (Recommended)

Download and install the official Vulkan SDK from LunarG:
https://vulkan.lunarg.com/sdk/home

After installation:
```bash
# Add to your ~/.zshrc or ~/.bash_profile:
export VULKAN_SDK="$HOME/VulkanSDK/<version>/macOS"
export PATH="$VULKAN_SDK/bin:$PATH"
export DYLD_LIBRARY_PATH="$VULKAN_SDK/lib:$DYLD_LIBRARY_PATH"
export VK_ICD_FILENAMES="$VULKAN_SDK/share/vulkan/icd.d/MoltenVK_icd.json"
export VK_LAYER_PATH="$VULKAN_SDK/share/vulkan/explicit_layer.d"
```

Then reload your shell:
```bash
source ~/.zshrc  # or ~/.bash_profile
```

### Option 2: Homebrew

```bash
brew install molten-vk
```

Then set environment variables:
```bash
export DYLD_LIBRARY_PATH="/opt/homebrew/lib:$DYLD_LIBRARY_PATH"
```

### Option 3: Manual MoltenVK

1. Download MoltenVK from https://github.com/KhronosGroup/MoltenVK/releases
2. Extract and place `libMoltenVK.dylib` in `/usr/local/lib/`
3. Create symlink:
```bash
sudo ln -s /usr/local/lib/libMoltenVK.dylib /usr/local/lib/libvulkan.dylib
```

## Verifying Installation

After installation, verify Vulkan is working:

```bash
# Check if library is found
ls -l $VULKAN_SDK/lib/libvulkan*.dylib

# Test metalshader
./target/release/metalshader example
```

## Current Status

- ✅ Compiles on macOS (Apple Silicon and Intel)
- ✅ Runs in headless mode (renders but doesn't display)
- 🚧 Windowed mode not yet implemented (requires winit event loop integration)

## Adding Windowed Support

The current implementation runs headless. To add proper window support:

1. Refactor main.rs to use winit's event loop
2. Add Vulkan surface creation from the window
3. Implement swapchain-based rendering (see metalshade.cpp for reference)
4. Integrate keyboard/mouse input from winit

Reference: `/opt/3d/metalshade/metalshade.cpp` - full GLFW+Vulkan implementation

## Limitations

- No window display (headless rendering only)
- LINEAR tiling may not be optimal on macOS (swapchain would be better)
- Mouse input not implemented
- Feedback buffers not implemented

## Future Improvements

- [ ] Add winit event loop to main.rs
- [ ] Create macOS-specific renderer with swapchain support
- [ ] Add mouse input handling
- [ ] Add window resizing support
- [ ] Add fullscreen toggle
- [ ] Port additional features from metalshade.cpp (pan, zoom, feedback buffers)
