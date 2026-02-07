
## 2026-02-08: Fixed unwanted panning while preserving zoom-at-cursor

**Problem:** View was panning whenever mouse moved, even without dragging. The zoom-at-cursor feature was causing this.

**Root cause:** Shader used `deltaMouse = mouse - referenceMouse` which changed on every frame as mouse moved. The formula simplified to using current `mouse` position, so any mouse movement caused the center to shift.

**Solution:** Added `iZoomMouse` uniform that stores mouse position when zooming starts.

**Implementation:**
1. **Rust viewer:** Added `zoom_mouse_x/y` fields that only update when scrolling happens (MouseWheel, +/- keys)
2. **Shader:** Use fixed `iZoomMouse` instead of current `mouse` in zoom formula
3. **Result:** Zoom centers on cursor when you scroll, but moving mouse without scrolling doesn't pan the view

**Files modified:**
- `/opt/3d/metalshader/src/main_macos.rs`
- `/opt/3d/shaders/mandelbrot_simple.frag`
