# Redox OS Testing Guide for Metalshader

## Prerequisites

1. **Redox toolchain** - The standard Rust toolchain doesn't include Redox targets
2. **Redox OS VM** - Running Redox with virtio-gpu and Venus support
3. **9P filesystem share** - For transferring binaries to Redox

## Building for Redox

### Option 1: Using Redox's build system

The proper way to build for Redox is using the Redox build system:

```bash
cd /opt/other/redox
make ARCH=aarch64
```

Then add metalshader as a recipe in the Redox build system.

### Option 2: Manual cross-compilation (if Redox Rust is set up)

```bash
cd /opt/other/qemu/guest-demos/metalshader
./build-redox.sh
```

This requires the Redox Rust toolchain to be properly configured.

## Running on Redox

### 1. Boot Redox OS

```bash
cd /opt/other/redox
./run-dev.sh  # or appropriate run script
```

### 2. Access the binary

If using 9P share:
```bash
# In Redox
ls /scheme/9p.hostshare/
/scheme/9p.hostshare/metalshader
```

If built as part of Redox:
```bash
# Binary should be in /bin or appropriate location
/bin/metalshader
```

### 3. Run metalshader

```bash
# Navigate to shader directory first (if needed)
cd /path/to/shaders

# Run metalshader
/scheme/9p.hostshare/metalshader example
```

## Expected Output

### Success Case

```
Available modes: 9 total
  [1] 1920x1080
  [2] 1280x720
  ...
Selected mode: [1] 1920x1080
Display path: display.virtio-gpu:2.0/1920/1080
Display resolution: 1920x1080
Framebuffer mapped at 0x..., size ...
Input device opened: input:consumer

Metalshader on Apple M4 Max (1920x1080)

[Animated shader should be visible on screen]

5.0s: 300 frames (60.0 FPS) - example
10.0s: 600 frames (60.0 FPS) - example
```

### Common Issues

#### 1. Display scheme not found

```
Error: Failed to open display scheme: No such file or directory
```

**Solution**: Ensure virtio-gpud is running:
```bash
ps | grep virtio-gpu
```

#### 2. Input scheme not found

```
Error: Failed to open input scheme: No such file or directory
```

**Solution**: Ensure inputd is running:
```bash
ps | grep inputd
```

#### 3. Resolution parsing fails

```
Warning: Could not parse resolution from path '...', using default 1920x1080
```

This is non-fatal - metalshader will use the default resolution.

#### 4. mmap fails

```
Error: mmap failed: ...
```

**Possible causes**:
- Insufficient memory
- Display driver issue
- Need to check virtio-gpu driver logs

## Keyboard Controls (Same as Linux)

- **Arrow Left/Right**: Switch between shaders
- **1-9**: Change resolution mode (may not work on Redox - not implemented)
- **ESC/Q**: Quit
- **F**: Toggle fullscreen (may not work - host-specific)

## Debugging

### Enable Redox kernel logs

```bash
# On host, QEMU should show kernel messages
# Look for virtio-gpu and inputd messages
```

### Check scheme availability

```bash
# In Redox
ls /scheme/
ls /scheme/input/
ls /scheme/display.virtio-gpu:2.0/
```

### Verify virtio-gpu driver

```bash
# In Redox
cat /scheme/sys:uname
ps aux | grep virtio
```

### Test simple display write

Before running metalshader, test basic display access:

```bash
# Try opening display
cat /scheme/display.virtio-gpu:2.0
```

## Performance Expectations

Based on Alpine Linux results (700+ FPS):

- **Expected on Redox**: 500-700 FPS
- **If < 100 FPS**: Likely GPU acceleration issue
- **If < 10 FPS**: Falling back to software rendering (bad)

## Differences from Linux

### What's the Same
- Vulkan renderer (100% identical)
- Shader format and loading
- Frame rendering logic
- Keyboard event types

### What's Different
- Display API: DRM ioctl → Redox scheme messages
- Input API: evdev → orbclient Events
- Scan codes: Linux key codes → PS/2 scan codes
- No resolution switching (not implemented)

## Troubleshooting Steps

1. **Verify display driver**:
   ```bash
   ls /scheme/ | grep display
   ```

2. **Check if running on correct VT**:
   ```bash
   # metalshader uses VT 2 by default
   # Make sure you're on VT 2
   ```

3. **Test with minimal shader**:
   - Use the simplest shader first
   - Helps isolate display vs. rendering issues

4. **Check Vulkan support**:
   - Redox should have Venus driver
   - `vkcube --wsi display` should work

5. **Monitor kernel messages**:
   - Look for virtio-gpu errors
   - Check for memory allocation failures

## Next Steps After Success

Once basic rendering works:

1. **Test all shaders** - Switch between shaders with arrow keys
2. **Test resolution switching** - Currently returns error, needs V2 API
3. **Performance profiling** - Compare FPS with Alpine
4. **Memory usage** - Check for leaks over time
5. **Stability** - Leave running for extended period

## Files to Check

If something goes wrong, examine these files:

- `/opt/other/redox/recipes/core/base/source/drivers/graphics/virtio-gpud/` - Display driver
- `/opt/other/redox/recipes/core/base/source/drivers/inputd/` - Input daemon
- Redox kernel logs (if available)
- QEMU console output

## Advanced: Adding Redox Recipe

To build metalshader as part of Redox:

1. Create recipe at `/opt/other/redox/cookbook/recipes/demos/metalshader/recipe.toml`
2. Add build instructions
3. Add to default packages
4. Rebuild Redox

This ensures metalshader is available in the standard Redox image.
