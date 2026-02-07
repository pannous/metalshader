# Redox OS Adaptation Guide for Metalshader

## Key Differences: Alpine Linux vs Redox OS

### Architecture Philosophy
- **Linux (Alpine)**: Monolithic kernel, drivers in kernel space, POSIX API
- **Redox**: Microkernel, drivers as userspace processes, scheme-based communication

### Display/Graphics Stack

#### Alpine Linux (Current Implementation)
```
Application
    ↓
DRM/GBM API (libdrm, libgbm)
    ↓
Kernel DRM Driver (/dev/dri/card0)
    ↓
virtio-gpu kernel module
    ↓
QEMU/Host GPU
```

#### Redox OS (Target)
```
Application
    ↓
Scheme API (file://display:*, graphics:*)
    ↓
virtio-gpu-venusd (userspace driver)
    ↓
virtio-core (Redox virtio library)
    ↓
QEMU/Host GPU
```

### Redox Graphics Drivers

Located in: `/opt/other/redox/recipes/core/base/source/drivers/graphics/`

1. **virtio-gpu-venusd** - Venus (Vulkan) support
   - Implements Venus protocol for Vulkan forwarding
   - Uses `driver-graphics` trait for DRM-like interface
   - Provides framebuffer and display management
   - Supports resource blobs for zero-copy

2. **virtio-gpud** - Basic virtio-gpu (2D/virgl)

3. **fbcond** - Framebuffer console

4. **vesad** - VESA graphics

### Redox Input Drivers

Located in: `/opt/other/redox/recipes/core/base/source/drivers/input/`

1. **ps2d** - PS/2 keyboard and mouse
   - Implements keyboard event handling
   - Uses scheme-based event delivery
   - File: `recipes/core/base/source/drivers/input/ps2d/src/scheme.rs`

2. **usbhidd** - USB HID devices

### Scheme-Based I/O

Redox uses "schemes" instead of traditional Unix device files:

| Purpose | Linux | Redox |
|---------|-------|-------|
| Display | `/dev/dri/card0` | `display:*` or `graphics:*` |
| Keyboard | `/dev/input/event*` | `input:*` or `event:*` |
| Files | `/path/to/file` | `file:/path/to/file` |

### Key APIs to Replace

#### Display (DRM/GBM → Redox Graphics Scheme)

**Linux Code:**
```rust
// Open DRM device
let drm_file = File::open("/dev/dri/card0")?;

// Get connector, mode
let connector = drm::control::Device::get_connector(...)?;
let mode = connector.modes().get(0)?;

// Create GBM buffer
let gbm_device = GbmDevice::new(drm_file)?;
let bo = gbm_device.create_buffer_object(...)?;

// Present frame
gbm_bo_map(...);
memcpy(frame_data);
gbm_bo_unmap(...);
drm_mode_dirty_fb(...);
```

**Redox Alternative (to be implemented):**
```rust
// Open graphics scheme
let display = File::open("display:0")?;

// Query display capabilities via ioctl-like interface
let mode_info = display.get_mode_info()?;

// Create framebuffer via scheme
let fb = display.create_framebuffer(width, height)?;

// Present frame
fb.write(frame_data)?;
fb.flush()?;  // Trigger display update
```

#### Input (Linux input-events → Redox Scheme)

**Linux Code:**
```rust
// Open event device
let kbd = File::open("/dev/input/event0")?;

// Read input event
let mut event = InputEvent::default();
file.read_exact(&mut event)?;

match event.kind {
    EventKind::Key(Key::Left) => { /* handle */ }
    ...
}
```

**Redox Alternative (to be implemented):**
```rust
// Open input scheme
let kbd = File::open("input:keyboard")?;

// Read event
let mut buf = [0u8; 64];
let n = kbd.read(&mut buf)?;

// Parse Redox input event format
let event = parse_input_event(&buf[..n])?;
match event {
    InputEvent::KeyPress(KeyCode::Left) => { /* handle */ }
    ...
}
```

### Vulkan on Redox

**Good news**: Vulkan (via ash) should work mostly unchanged!

- Vulkan is a cross-platform API
- `ash` crate provides platform-agnostic bindings
- Venus protocol is supported via `virtio-gpu-venusd`
- May need to ensure Vulkan loader finds the right ICD

**Potential issues**:
- Vulkan loader path: `/usr/share/vulkan/icd.d/` on Linux
- Redox may use different paths or scheme-based discovery
- Check: `recipes/core/base/source/drivers/graphics/virtio-gpu-venusd/src/venus.rs`

### Implementation Strategy

#### Phase 1: Minimal Viable Port (Linux-only testing)
✅ **DONE** - Current state:
- Rust conversion complete
- Modular design with platform abstraction
- Conditional compilation via `cfg(target_os = "linux")`
- Test on Alpine VM first

#### Phase 2: Redox Display Backend
Create `src/display_redox.rs`:
```rust
pub struct Display {
    scheme_fd: File,
    width: u32,
    height: u32,
}

impl Display {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Open display scheme
        let scheme_fd = File::open("display:0")?;

        // Query resolution via custom ioctl or read/write protocol
        // (need to study driver-graphics trait in Redox)

        Ok(Self { scheme_fd, width, height })
    }

    pub fn present(&mut self, frame_data: &[u8]) -> Result<(), ...> {
        // Write frame to scheme
        self.scheme_fd.write_all(frame_data)?;
        // Flush/commit
        ...
    }
}
```

#### Phase 3: Redox Input Backend
Create `src/input_redox.rs`:
```rust
pub struct KeyboardInput {
    event_fd: File,
}

impl KeyboardInput {
    pub fn new() -> Result<Self, ...> {
        let event_fd = File::open("input:keyboard")?;
        // Set non-blocking
        ...
        Ok(Self { event_fd })
    }

    pub fn poll_event(&mut self) -> Option<KeyEvent> {
        // Read from scheme
        // Parse Redox event format
        ...
    }
}
```

#### Phase 4: Integration & Testing
1. Update `src/main.rs` to select backend at compile time:
   ```rust
   #[cfg(target_os = "linux")]
   use crate::display::Display;

   #[cfg(target_os = "redox")]
   use crate::display_redox::Display;
   ```

2. Build for Redox:
   ```bash
   # Cross-compile from host
   cargo build --target aarch64-unknown-redox --release

   # Or build on Redox guest
   # (requires Rust toolchain on Redox)
   ```

3. Test on Redox VM:
   ```bash
   # Copy to 9P share
   cp target/aarch64-unknown-redox/release/metalshader \
      /opt/other/redox/share/

   # Run Redox VM
   cd /opt/other/redox && ./run-dev.sh

   # Inside Redox:
   /scheme/9p.hostshare/metalshader cube
   ```

### Research TODO

1. **Study `driver-graphics` trait**:
   - Location: `recipes/core/base/source/drivers/graphics/driver-graphics/`
   - Understand DRM-like interface provided to applications
   - Check if there's a userspace library for graphics

2. **Check Redox Vulkan support**:
   - Look for Venus capset negotiation in `virtio-gpu-venusd`
   - Verify Vulkan ICD path/discovery mechanism
   - Test basic `vkcube` on Redox if available

3. **Study input event format**:
   - Check `inputd` daemon: `recipes/core/base/source/drivers/inputd/`
   - See how events are delivered to applications
   - Look for existing Redox apps that use keyboard input

4. **Memory mapping**:
   - Redox may handle DMA/shared memory differently
   - Check if zero-copy is possible via schemes
   - May need to use different approach than `mmap()`

### Testing Plan

1. **Alpine Linux** (current target):
   - Build natively on guest: `./build-rust.sh`
   - Test with existing shaders
   - Verify feature parity with C version

2. **Redox OS** (future target):
   - Start Redox VM: `/opt/other/redox/run-dev.sh`
   - Copy binary via 9P share
   - Test basic Vulkan functionality
   - Adapt display/input as needed

### Files to Create

- `src/display_redox.rs` - Redox display backend
- `src/input_redox.rs` - Redox input backend
- `.cargo/config.toml` - Cross-compilation config for Redox
- `build-redox.sh` - Build script for Redox target

### Compatibility Strategy

Use **trait-based abstraction** for maximum portability:

```rust
trait DisplayBackend {
    fn new() -> Result<Self, Box<dyn Error>> where Self: Sized;
    fn get_resolution(&self) -> (u32, u32);
    fn present(&mut self, data: &[u8]) -> Result<(), Box<dyn Error>>;
}

trait InputBackend {
    fn new() -> Result<Self, Box<dyn Error>> where Self: Sized;
    fn poll_event(&mut self) -> Option<KeyEvent>;
}

#[cfg(target_os = "linux")]
type Display = LinuxDisplay;

#[cfg(target_os = "redox")]
type Display = RedoxDisplay;
```

This allows:
- Single codebase for both platforms
- Easy testing on Alpine before Redox port
- Future platform additions (e.g., native macOS Metal backend)

## Next Steps

1. ✅ Test Rust version on Alpine VM
2. ⏳ Explore Redox graphics scheme API in detail
3. ⏳ Implement `display_redox.rs` and `input_redox.rs`
4. ⏳ Cross-compile and test on Redox
5. ⏳ Document any Vulkan-specific adaptations needed
