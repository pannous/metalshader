# Metalshader - Interactive Shader Viewer

Interactive shader viewer with keyboard navigation for ShaderToy-style GLSL shaders.

## Features

- **Simple shader selection**: Just specify the base name without extensions
- **Arrow key navigation**: Switch between shaders in real-time
- **Auto-discovery**: Automatically finds all compiled shaders in the shaders directory
- **Texture support**: Provides binding 0 (UBO) and binding 1 (sampler2D)
- **Live reload**: Hot-swap shaders without restarting

## Usage

```bash
./metalshader <shader_name>
```

Example:
```bash
./metalshader bumped_sinusoidal_warp
```

The program will automatically find:
- `/root/metalshade/shaders/bumped_sinusoidal_warp.vert.spv`
- `/root/metalshade/shaders/bumped_sinusoidal_warp.frag.spv`

## Controls

- **Arrow Left**: Previous shader
- **Arrow Right**: Next shader
- **1-9**: Change resolution mode (displays available modes on startup)
- **F**: Toggle fullscreen (host-dependent)
- **ESC** or **Q**: Quit

## Building

### For Linux (Alpine, Debian, etc.)

```bash
cargo build --release
```

The binary will be at `target/release/metalshader`.

### For macOS

```bash
cargo build --release
```

**Prerequisites**: Install Vulkan SDK or MoltenVK:
```bash
# Option 1: Download Vulkan SDK from https://vulkan.lunarg.com/sdk/home
# Option 2: Install via Homebrew
brew install molten-vk
export DYLD_LIBRARY_PATH="/opt/homebrew/lib:$DYLD_LIBRARY_PATH"
```

See `notes/macos-setup.md` for detailed setup instructions.

**Current Status**: Headless mode only (renders but doesn't display). Windowed support in development.

The binary will be at `target/release/metalshader`.

### For Redox OS

```bash
./build-redox.sh
```

**Note**: Redox builds require the Redox Rust toolchain. See `notes/redox-testing-guide.md` for detailed instructions.

Alternatively, build metalshader as part of the Redox build system by creating a recipe.

## Compiling Shaders

Shaders must be pre-compiled to SPIR-V:

```bash
cd /root/metalshade/shaders

# Compile a shader pair
glslangValidator -V your_shader.vert -o your_shader.vert.spv
glslangValidator -V your_shader.frag -o your_shader.frag.spv

# Then run with:
./metalshader your_shader
```

## Shader Requirements

Your shaders should use the standard ShaderToy uniform layout:

### Vertex Shader (*.vert)
```glsl
#version 450

layout(location = 0) out vec2 fragCoord;

layout(binding = 0, set = 0) uniform UniformBufferObject {
    vec3 iResolution;  // viewport resolution (in pixels)
    float iTime;       // shader playback time (in seconds)
    vec4 iMouse;       // mouse pixel coords
} ubo;

void main() {
    // Generate fullscreen quad
    vec2 positions[6] = vec2[](
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0)
    );
    gl_Position = vec4(positions[gl_VertexIndex], 0.0, 1.0);
    fragCoord = (positions[gl_VertexIndex] * 0.5 + 0.5) * ubo.iResolution.xy;
}
```

### Fragment Shader (*.frag)
```glsl
#version 450

layout(location = 0) in vec2 fragCoord;
layout(location = 0) out vec4 fragColor;

layout(binding = 0, set = 0) uniform UniformBufferObject {
    vec3 iResolution;
    float iTime;
    vec4 iMouse;
} ubo;

layout(binding = 1, set = 0) uniform sampler2D iChannel0;  // Optional texture

void main() {
    vec2 uv = fragCoord / ubo.iResolution.xy;

    // Your shader code here
    fragColor = vec4(uv.x, uv.y, abs(sin(ubo.iTime)), 1.0);
}
```

## Architecture

- **Platform abstraction**: Unified code works on both Linux and Redox
- **Display**: DRM/KMS (Linux) or Redox graphics schemes (Redox)
- **Input**: evdev (Linux) or orbclient Events (Redox)
- **Vulkan Venus**: GPU acceleration via virtio-gpu on both platforms
- **Linear tiling + HOST_VISIBLE memory**: CPU-accessible images
- **Procedural texture**: 256x256 RGBA checkerboard at binding 1
- **Live shader reload**: Pipelines recreated on arrow key press

### Supported Platforms

- ✅ **Linux** (Alpine, Ubuntu, etc.) - Fully working with DRM/KMS
- ✅ **macOS** - Compiles and runs (headless mode, window support in development)
- 🚧 **Redox OS** - Implementation complete, pending testing

## Performance

Tested on Virtio-GPU Venus (Apple M2 Pro):
- **306 FPS** with complex warp shaders
- **405 FPS** with simple gradient shaders
- **No fence hangs** or ring errors
- **Instant shader switching** with arrow keys

## Directory Structure

```
/root/metalshade/
├── shaders/
│   ├── bumped_sinusoidal_warp.frag      # Source GLSL
│   ├── bumped_sinusoidal_warp.vert      # Source GLSL
│   ├── bumped_sinusoidal_warp.frag.spv  # Compiled SPIR-V
│   ├── bumped_sinusoidal_warp.vert.spv  # Compiled SPIR-V
│   ├── plasma.frag
│   ├── plasma.vert.spv
│   └── ...
└── metalshader                           # This program
```

## Troubleshooting

### No shaders found
```
No compiled shaders found in /root/metalshade/shaders
```
**Solution**: Compile your shaders with `glslangValidator -V`

### Keyboard not detected
```
Warning: No keyboard input found, arrow key navigation disabled
```
**Solution**: Run from a console with `/dev/input/event*` access. The program will still render but without navigation.

### Blue screen or solid color
**Check binding numbers**: Your shader's uniform buffer must be at `binding = 0`. If using textures, they should be at `binding = 1`.

## Comparison to shadertoy_viewer

| Feature | shadertoy_viewer | metalshader |
|---------|------------------|-------------|
| Shader selection | Full path with .spv | Base name only |
| Navigation | No (fixed duration) | Yes (arrow keys) |
| Live reload | No | Yes |
| Auto-discovery | No | Yes |
| Duration | Fixed | Infinite loop |

## Example Session

```bash
# Start with plasma shader
./metalshader plasma

# Output:
# Found 9 compiled shader(s)
#   [0] bumped_sinusoidal_warp
#   [1] clouds_bookofshaders
#   [2] creation_by_silexars
#   [3] dolphin_4sS3zG
#   [4] plasma
#   [5] simple_gradient
#   [6] tunnel
# Starting with shader: plasma
# Metalshader on Virtio-GPU Venus (1280x800)
# Loaded shader: plasma
# 1.0s: 60 frames (405 FPS) - plasma
# <Press arrow right>
# >> Next shader: simple_gradient
# Loaded shader: simple_gradient
# <Press ESC to quit>
```
