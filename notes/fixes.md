
## 2026-02-08: Fixed unwanted panning while preserving zoom-at-cursor

**Problem:** View was panning whenever mouse moved, even without dragging. The zoom-at-cursor feature was causing this.

**Root cause:** Shader used `deltaMouse = mouse - referenceMouse` which changed on every frame as mouse moved. The formula simplified to using current `mouse` position, so any mouse movement caused the center to shift.

**Failed approach #1:** Store mouse position when zooming and use that in shader.
- **Issue:** Broke fixed-point zooming because it used stored position instead of current cursor position.
- **Also broke:** Drag-and-drop positioning.

**Final solution:** Move zoom-at-cursor logic to Rust viewer code.

**Why this works:**
- Shaders can't implement proper fixed-point zooming because they lack state (can't remember previous zoom level)
- Need to detect zoom changes and calculate center adjustment only when zoom actually changes
- Viewer tracks `old_zoom` and `new_zoom`, calculates: `center_adjustment = (mouse - 0.5) * viewport * (1/old_zoom - 1/new_zoom)`
- Accumulates adjustments in `zoom_center_x/y`, passes to shader as simple offset

**Implementation:**
1. **Rust viewer:** Track zoom level, calculate center adjustment when zoom changes (MouseWheel, +/- keys)
2. **Shader:** Just add `iZoomMouse` offset to center (no calculation)
3. **Result:**
   - ✅ Fixed-point zooming works (cursor stays fixed when zooming)
   - ✅ No panning when moving mouse (adjustment only on zoom change)
   - ✅ Drag-and-drop works correctly

**Files modified:**
- `/opt/3d/metalshader/src/main_macos.rs`
- `/opt/3d/shaders/mandelbrot_simple.frag`
