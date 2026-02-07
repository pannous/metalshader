# Refactor Plan: Generic Viewer + Self-Contained Shaders

## Problem
Currently the viewer has hard-coded logic that detects shader types by name ("autozoom") and applies different zoom calculations. This violates separation of concerns - the viewer should be generic.

## Goal
- **Viewer**: Generic, reusable, no shader-specific logic
- **Shaders**: Self-contained, specify their own behavior

## Current Issues

### 1. Zoom Calculation in Viewer
```rust
let is_autozoom = self.shader_manager.get(self.current_shader_idx)
    .map(|s| s.name.contains("autozoom"))
    .unwrap_or(false);
let current_zoom = if is_autozoom {
    (effective_time * ZOOM_SPEED).exp()
} else {
    (self.scroll_y * 0.1).exp().sqrt()
};
```
**Problem**: Viewer decides zoom formula based on shader name.

### 2. Mouse Smoothing Calibration
Mouse smoothing uses zoom to calculate lerp factor, but zoom formula is shader-specific.

## Solution

### Option A: Pass Both Raw and Smoothed Mouse
- Viewer always smooths mouse with a **generic** formula
- Pass both `iMouse` (smoothed) and `iMouseRaw` (raw) to shader
- Shader chooses which to use based on its needs

**Pros**: Simple, backward compatible
**Cons**: Smoothing might not be calibrated correctly for all zoom styles

### Option B: Shader-Controlled Smoothing
- Viewer passes `iMouseRaw` only
- Shader implements its own smoothing using previous frame feedback
- Requires feedback buffers (ping-pong)

**Pros**: Complete shader control
**Cons**: More complex, requires feedback rendering system

### Option C: Uniform-Based Configuration
- Add shader metadata in comments:
  ```glsl
  // @zoom_formula: manual  // or "auto"
  // @zoom_speed: 0.15
  ```
- Viewer parses metadata and configures itself
- Still violates separation but at least it's declarative

**Cons**: Parser complexity, fragile

### Option D: Generic Fixed Smoothing (Recommended)
- Viewer applies **fixed, zoom-independent** mouse smoothing
- Simple exponential smooth with constant factor:
  ```rust
  const MOUSE_SMOOTH_FACTOR: f32 = 0.1; // 10% per frame
  mouse_smooth += (mouse - mouse_smooth) * MOUSE_SMOOTH_FACTOR * delta_time * 60.0;
  ```
- Good enough for most use cases
- Shaders can request raw mouse if needed via iMouse.zw or separate uniform

**Pros**: Simple, generic, no shader detection
**Cons**: Not perfectly calibrated for extreme zoom, but acceptable

## Recommended Implementation

### 1. Remove Zoom Calculation from Viewer
- Delete all `current_zoom` calculation in application
- Delete shader name detection logic
- Viewer doesn't care about zoom at all

### 2. Simple Generic Mouse Smoothing
```rust
// Fixed smoothing, ~200ms time constant
const SMOOTH_FACTOR: f32 = 0.1;
let smooth_amount = (SMOOTH_FACTOR * delta_time * 60.0).min(1.0);
self.mouse_smooth_x += (self.mouse_x - self.mouse_smooth_x) * smooth_amount;
self.mouse_smooth_y += (self.mouse_y - self.mouse_smooth_y) * smooth_amount;
```

### 3. Shader Decides Everything
Each shader calculates its own zoom and chooses mouse handling:
```glsl
float zoom = calculateZoom(); // shader-specific
vec2 mouse = ubo.iMouse.xy;   // smoothed by viewer
// Or: vec2 mouseRaw = ubo.iMouseRaw.xy; // if we add this uniform
```

### 4. Optional: Expose Raw Mouse
Add to UBO:
```rust
i_mouse_raw: [f32; 2],  // Unsmoothed mouse position
```

Then shaders can choose:
```glsl
vec2 mouse = ubo.iMouse.xy;      // Smoothed (default)
vec2 mouseRaw = ubo.iMouseRaw;   // Raw if needed
```

## Migration Plan

1. **Phase 1**: Remove zoom calculation from viewer
   - Replace with fixed smoothing constant
   - Keep same UBO structure

2. **Phase 2**: Test with existing shaders
   - Verify smoothing still works acceptably
   - Adjust SMOOTH_FACTOR if needed

3. **Phase 3** (optional): Add iMouseRaw uniform
   - Let shaders choose raw vs smoothed

4. **Phase 4**: Update C version to match
   - Same generic approach
   - Feature parity maintained

## Expected Outcome

**Before** (Current):
- Viewer: 50 lines of shader-specific logic
- Shaders: Simple, rely on viewer

**After** (Goal):
- Viewer: 5 lines of generic smoothing
- Shaders: Self-contained, explicit about behavior
- More maintainable, easier to add new shaders

## Next Steps

1. Implement Option D (generic fixed smoothing) in Rust
2. Remove shader detection logic
3. Test with mandelbrot_simple and mandelbrot_autozoom
4. If smoothing is insufficient, add iMouseRaw uniform
