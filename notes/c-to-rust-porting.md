# C to Rust Porting Status

Last synced with C version: 2026-02-07

## ✅ Ported Features (from last 20 commits)

### Mouse and Input Controls
- **Drag-and-drop panning** (commit 3bd4cca)
  - Left click + drag to pan view
  - Pan offset accumulates on mouse release
  - Persistent across multiple drags

- **Keyboard zoom controls** (commit 14e3294)
  - `+` or `=` key: Zoom in
  - `-` key: Zoom out
  - Numpad `+/-` also work
  - Works alongside mouse wheel zoom

- **Scroll clamping** (commit 6d715ab)
  - Scroll values clamped to [-100, 100]
  - Prevents exp() from producing extreme values
  - Fixes zoom controls stopping issue

- **Reset key** (commit 6d715ab, 3bd4cca)
  - `R` key resets zoom and pan to defaults
  - Clears both scroll offset and pan offset

### UBO (Uniform Buffer Object)
- **Added i_scroll[2]** - Accumulated scroll offset for zoom
- **Added i_pan[2]** - Accumulated pan offset for drag-and-drop

### Shader Path Resolution
- **Auto-detect shader extension** (commit 20001d6)
  - Tries `.frag`, `.fsh`, `.glsl` in order
  - Handles missing extensions: `./shaders/mandelbrot` → `./shaders/mandelbrot.frag`
  - Strips trailing dots if present

## ❌ Not Ported (Major Features)

### Feedback Rendering System (commits eabf576, 60981f5, 5ca7707)
**Why not ported:** Requires major renderer architecture changes

This is a complete rewrite of the rendering pipeline:
- Ping-pong feedback buffers
- Render to texture instead of swapchain
- Blit feedback buffer to swapchain for display
- Dynamic descriptor updates
- Layout transitions: COLOR_ATTACHMENT ↔ SHADER_READ

**Benefits:**
- Enables persistent effects (paint brush strokes)
- Continuous evolution (organic_life patterns)
- True feedback loops (flowing_colors)

**Implementation effort:**
- ~200+ lines of Vulkan code
- New framebuffer infrastructure
- Modified command buffer recording
- Descriptor set updates per frame

**Recommendation:** Port this as a separate major feature when needed for feedback shaders.

## 📝 Other Commits (Shader-only)

These commits only affect shaders (not the renderer):
- commit fecd400: Add kaleidoscope shader
- commit 2cd4508: Educational Mandelbrot shader
- commit 9ba2860: Improve mainImage conversion
- commit 89a91f7: Twitter shader converter fixes
- commit 9436550: Twitter/code-golf converter
- commit e49bc41: Feedback test shaders
- commit 025efee: Mouse trace shader
- commit 2952e51: Button duration demo
- commit a68e96c: Button duration fixes

## 🎯 Current Feature Parity

The Rust version now has **feature parity** with the C version for:
- ✅ Window management (windowed, fullscreen, resolution switching)
- ✅ Mouse input (panning, zoom, position tracking)
- ✅ Keyboard controls (navigation, zoom, reset)
- ✅ Shader compilation and hot-reloading
- ✅ UBO data (resolution, time, mouse, scroll, pan)

**Missing:**
- ❌ Feedback rendering system (ping-pong buffers)
- ❌ Multi-button mouse tracking (right/middle/button4/button5)
- ❌ Button press duration tracking

## 📊 Commit Comparison

C version commits analyzed: last 20 (fecd400 to a68e96c)
Features ported: 6/9 major features (67%)
Lines changed: ~150 lines in main_macos.rs

## 🚀 Next Steps

If feedback rendering is needed:
1. Study C implementation in commits eabf576, 5ca7707
2. Design Rust architecture for ping-pong buffers
3. Modify SwapchainRenderer to support feedback path
4. Add descriptor set updates per frame
5. Test with feedback shaders (organic_life, paint)
