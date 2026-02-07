
## 2026-02-08: Fixed unwanted panning in mandelbrot_simple

**Problem:** View was panning whenever mouse moved, even without dragging.

**Root cause:** Shader had "zoom-at-cursor" logic (lines 95-104) that calculated `deltaMouse = mouse - referenceMouse` on every frame. This caused the center to shift based on mouse movement, not just during drag operations.

**Fix:** Removed the zoom-at-cursor section entirely. View now only pans during actual drag operations (when `iMouse.z > 0`).

**File modified:** `/opt/3d/shaders/mandelbrot_simple.frag`
