# Redox OS API Mapping for Metalshader

## Executive Summary

**Key Finding**: Redox uses a fundamentally different I/O model than Linux:
- **Linux**: Uses special files (`/dev/input/eventX`, `/dev/dri/cardX`) with ioctl system calls
- **Redox**: Uses scheme-based I/O where everything is a message-passing protocol

## Graphics/Display API Mapping

### Linux DRM/KMS (Current Implementation)

**File**: `src/display.rs`

```rust
// Linux approach
let file = File::open("/dev/dri/card0")?;
ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &mut res);
ioctl(fd, DRM_IOCTL_MODE_SETCRTC, &crtc_req);
mmap(fd, size, PROT_READ | PROT_WRITE, MAP_SHARED, offset);
```

**Key Linux APIs**:
- `/dev/dri/cardX` - Direct rendering interface
- `DRM_IOCTL_MODE_GETRESOURCES` - Query displays
- `DRM_IOCTL_MODE_GETCONNECTOR` - Get connector info
- `DRM_IOCTL_MODE_CREATE_DUMB` - Create framebuffer
- `DRM_IOCTL_MODE_MAP_DUMB` - Map framebuffer to memory
- `mmap()` - Memory map the framebuffer
- `DRM_IOCTL_MODE_RMFB` - Destroy framebuffer

### Redox Graphics Scheme (Target)

**Path**: `display:0` or `display.virtio-gpu:v2/<vt>`

**Architecture**:
```
Application (metalshader)
    ↓ (read/write/call to scheme)
driver-graphics crate (GraphicsScheme)
    ↓ (trait GraphicsAdapter)
virtio-gpud (VirtGpuAdapter)
    ↓ (virtio commands)
QEMU virtio-gpu device
    ↓ (Venus protocol)
Host Vulkan/MoltenVK
```

**Redox Graphics Trait** (`driver-graphics::GraphicsAdapter`):

```rust
trait GraphicsAdapter {
    fn display_count(&self) -> usize;
    fn display_size(&self, display_id: usize) -> (u32, u32);
    fn create_dumb_framebuffer(&mut self, width: u32, height: u32) -> Self::Framebuffer;
    fn map_dumb_framebuffer(&mut self, fb: &Self::Framebuffer) -> *mut u8;
    fn update_plane(&mut self, display_id: usize, fb: &Self::Framebuffer, damage: Damage);
}

trait Framebuffer {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
}
```

**Access Methods**:

1. **V1 API** (Simple, for Orbital):
   ```rust
   // Open: "display.virtio-gpu:<vt>.<screen>/width/height"
   let display = File::open("display.virtio-gpu:2.0/1920/1080")?;

   // Get framebuffer pointer via mmap
   let ptr = mmap(display.as_raw_fd(), ...);

   // Write damage region and flush
   let damage = Damage { x: 0, y: 0, width: 1920, height: 1080 };
   display.write(&damage_bytes)?;
   display.sync_all()?;  // fsync triggers update_plane
   ```

2. **V2 API** (DRM-compatible, for Mesa/Vulkan):
   ```rust
   // Open: "display.virtio-gpu:v2/<vt>"
   let display = File::open("display.virtio-gpu:v2/2")?;

   // Use graphics_ipc::v2::ipc protocol with call() syscall
   use graphics_ipc::v2::ipc::*;

   // Query card resources
   let mut res = DrmModeCardRes::new();
   call(fd, MODE_CARD_RES, res.as_mut_bytes())?;

   // Create dumb buffer
   let mut create = DrmModeCreateDumb::new();
   create.set_width(1920);
   create.set_height(1080);
   create.set_bpp(32);
   call(fd, MODE_CREATE_DUMB, create.as_mut_bytes())?;

   // Map buffer
   let mut map = DrmModeMapDumb::new();
   map.set_handle(create.handle());
   call(fd, MODE_MAP_DUMB, map.as_mut_bytes())?;

   let ptr = mmap(fd, size, MAP_SHARED, map.offset());

   // Update plane (present)
   let update = UpdatePlane {
       display_id: 0,
       fb_id: create.handle(),
       damage: Damage { x: 0, y: 0, width: 1920, height: 1080 },
   };
   call(fd, UPDATE_PLANE, &update_bytes)?;
   ```

**Key Differences**:
- Linux: `ioctl()` with numbered request codes
- Redox: `call()` syscall with typed message structs
- Linux: Device nodes in `/dev`
- Redox: Scheme paths like `display.virtio-gpu:v2/2`
- Both support mmap for zero-copy framebuffer access

**Damage Region Struct** (graphics_ipc::v2::Damage):
```rust
#[repr(C)]
pub struct Damage {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
```

## Input API Mapping

### Linux evdev (Current Implementation)

**File**: `src/input.rs`

```rust
// Linux approach
let file = File::open("/dev/input/event0")?;
fcntl(fd, F_SETFL, O_NONBLOCK);

// Read events
let mut event = InputEvent::zeroed();
file.read_exact(&event_bytes)?;

// InputEvent structure (Linux)
struct InputEvent {
    time: timeval,      // 16 bytes
    type_: u16,         // EV_KEY, EV_REL, etc.
    code: u16,          // KEY_LEFT, KEY_ESC, etc.
    value: i32,         // 0=release, 1=press, 2=repeat
}
// Total: 24 bytes on 64-bit
```

**Key Linux APIs**:
- `/dev/input/eventX` - Input event devices
- `EVIOCGNAME` ioctl - Get device name
- `EventKind::Key` - Keyboard events
- `Key::from_code()` - Convert scan codes to key names
- Non-blocking read via `O_NONBLOCK`

### Redox Input Scheme (Target)

**Path**: `input:consumer` (for reading events) or `input:producer` (for writing)

**Architecture**:
```
Keyboard/PS2 Driver (ps2d)
    ↓ (writes orbclient::Event)
input:producer
    ↓ (inputd multiplexer)
input:consumer (per VT)
    ↓ (reads orbclient::Event)
Application (metalshader)
```

**Event Format** (orbclient::Event from orbclient crate v0.3.27):

```rust
// This is from the orbclient crate, not Redox source
// Based on inputd usage pattern:
// - Written as raw bytes by ps2d
// - Read as orbclient::Event by consumers
// - Size: ~16-24 bytes (estimated, contains event type + data)

// From inputd/main.rs usage:
unsafe {
    core::slice::from_raw_parts(
        buf.as_ptr() as *const Event,
        buf.len() / size_of::<Event>(),
    )
}

// Event types from inputd code:
match event.to_option() {
    EventOption::Key(key_event) => {
        // key_event.scancode (u8) - scan code
        // key_event.pressed (bool) - press/release
    }
    EventOption::Resize(resize_event) => {
        // resize_event.width, height
    }
    _ => {}
}
```

**Redox Input Access**:

```rust
// Open consumer for reading input events
let input = File::open("input:consumer")?;

// Set non-blocking mode
use syscall::{fcntl, F_SETFL, O_NONBLOCK};
fcntl(input.as_raw_fd(), F_SETFL, O_NONBLOCK)?;

// Read events
let mut events = [Event::default(); 16];
let bytes_read = input.read(&event_bytes)?;
let event_count = bytes_read / size_of::<Event>();

// Parse events
for event in &events[..event_count] {
    match event.to_option() {
        EventOption::Key(key_event) => {
            if key_event.pressed {
                match key_event.scancode {
                    0x4B => /* Left arrow */,
                    0x4D => /* Right arrow */,
                    0x01 => /* ESC */,
                    0x10 => /* Q */,
                    0x21 => /* F */,
                    0x02..=0x0A => /* 1-9 keys */,
                    _ => {}
                }
            }
        }
        _ => {}
    }
}
```

**Scan Code Mapping** (PS/2 Set 1):
- `0x01` - ESC
- `0x10` - Q
- `0x21` - F (fullscreen toggle)
- `0x02` - KEY_1
- `0x03` - KEY_2
- ... (through 0x0A for KEY_9)
- `0x4B` - Left arrow
- `0x4D` - Right arrow
- `0x3B..=0x44` - F1-F10 (used for VT switching when Super is held)

**Key Differences**:
- Linux: Structured `InputEvent` with type/code/value fields
- Redox: `orbclient::Event` enum with specific event types
- Linux: Uses key code enums (KEY_LEFT, KEY_ESC)
- Redox: Uses raw PS/2 scan codes (0x4B, 0x01)
- Both: Support non-blocking reads
- Both: ~24 bytes per event

**VT (Virtual Terminal) Handling**:

On Redox, each `input:consumer` is associated with a VT number. The inputd daemon:
1. Routes events only to the active VT
2. Handles Super+F1-F12 for VT switching
3. Provides VT activation events via `input:handle/display/<device>`

For metalshader, we'll use a single VT and don't need VT switching logic.

## Required Redox Crates

### For Graphics (V2 DRM API):
```toml
[target.'cfg(target_os = "redox")'.dependencies]
graphics-ipc = { git = "https://gitlab.redox-os.org/redox-os/graphics-ipc" }
syscall = "0.2"  # For call() syscall
```

### For Input:
```toml
[target.'cfg(target_os = "redox")'.dependencies]
orbclient = "0.3.27"  # For Event types
syscall = "0.2"       # For fcntl and O_NONBLOCK
libredox = "0.2"      # For Redox-specific helpers (optional)
```

### Note on Scheme Access:

Unlike Linux where we need to find the right device (`/dev/input/event0` vs `event1`), Redox has well-known scheme paths:
- Graphics: `display.virtio-gpu:v2/2` (VT 2 is default)
- Input: `input:consumer` (automatically assigned a VT)

## Implementation Strategy

### For Display (Easier):

Use **V1 API** initially for simplicity:
```rust
impl RedoxDisplay {
    fn new() -> Result<Self, Box<dyn Error>> {
        // Open the display scheme
        let display = File::open("display.virtio-gpu:2.0")?;

        // Parse resolution from path (fpath returns full path)
        // Or query using GraphicsScheme methods

        // Map framebuffer
        let size = width * height * 4;
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                display.as_raw_fd(),
                0,
            )
        };

        Ok(RedoxDisplay { display, ptr, width, height })
    }

    fn present(&mut self, data: &[u8], row_pitch: usize) -> Result<(), Box<dyn Error>> {
        // Copy to mapped framebuffer
        unsafe {
            let fb = std::slice::from_raw_parts_mut(self.ptr as *mut u8, self.width * self.height * 4);

            for y in 0..self.height {
                let dst_off = y * self.width * 4;
                let src_off = y * row_pitch;
                fb[dst_off..dst_off + self.width * 4]
                    .copy_from_slice(&data[src_off..src_off + self.width * 4]);
            }
        }

        // Trigger flush
        let damage = Damage {
            x: 0,
            y: 0,
            width: self.width as u32,
            height: self.height as u32,
        };
        self.display.write(&damage_bytes)?;
        self.display.sync_all()?;  // fsync

        Ok(())
    }
}
```

**Pros**:
- Simpler API, fewer dependencies
- Direct mmap access like Linux
- Known to work (used by Orbital compositor)

**Cons**:
- Less featureful than V2 API
- Fixed resolution on open

### For Input (Straightforward):

```rust
impl RedoxInput {
    fn new() -> Result<Self, Box<dyn Error>> {
        let file = File::open("input:consumer")?;

        // Set non-blocking
        use syscall::{fcntl, F_SETFL, O_NONBLOCK};
        fcntl(file.as_raw_fd(), F_SETFL, O_NONBLOCK)?;

        Ok(RedoxInput { file })
    }

    fn poll_event(&mut self) -> Option<KeyEvent> {
        use orbclient::{Event, EventOption};

        let mut events = [Event::default(); 16];
        let event_bytes = unsafe {
            std::slice::from_raw_parts_mut(
                events.as_mut_ptr() as *mut u8,
                events.len() * std::mem::size_of::<Event>(),
            )
        };

        let bytes_read = match self.file.read(event_bytes) {
            Ok(n) => n,
            Err(_) => return None,
        };

        let count = bytes_read / std::mem::size_of::<Event>();

        for event in &events[..count] {
            if let EventOption::Key(key_event) = event.to_option() {
                if !key_event.pressed { continue; }

                match key_event.scancode {
                    0x4B => return Some(KeyEvent::Left),
                    0x4D => return Some(KeyEvent::Right),
                    0x21 => return Some(KeyEvent::Fullscreen),
                    0x01 | 0x10 => return Some(KeyEvent::Quit),
                    0x02..=0x0A => {
                        let mode = (key_event.scancode - 0x01) as u8;
                        if mode >= 1 && mode <= 9 {
                            return Some(KeyEvent::Resolution(mode));
                        }
                    }
                    _ => {}
                }
            }
        }

        None
    }
}
```

## Unknowns / To Investigate

1. **Resolution switching**: Does Redox V1 API support runtime resolution changes? May need V2 API.
2. **Framebuffer format**: Assumed BGRX (32-bit) - need to verify
3. **VT number**: Hardcoded VT 2 - should we auto-detect or make configurable?
4. **orbclient Event size**: Need to verify `size_of::<Event>()` at compile time
5. **Scan codes**: PS/2 Set 1 assumed - verify for VirtIO keyboard

## Testing Plan

Phase 1: **Stub compilation** (no Redox needed)
- Create stubs that compile for `target_os = "redox"`
- Verify cross-compilation works

Phase 2: **Redox boot with logging** (Redox VM required)
- Add extensive logging to each API call
- Boot Redox and run metalshader
- Capture logs to understand actual behavior

Phase 3: **Incremental fixes** (Redox VM required)
- Fix issues found in Phase 2
- Test display first (simpler)
- Then test input

## Summary Table

| Feature | Linux (Current) | Redox (Target) | Complexity |
|---------|----------------|----------------|------------|
| **Display Open** | `/dev/dri/card0` | `display.virtio-gpu:2.0` | Low |
| **Display Query** | `ioctl(DRM_IOCTL_MODE_GETRESOURCES)` | Parse from fpath or use V2 `call()` | Medium |
| **FB Create** | `ioctl(DRM_IOCTL_MODE_CREATE_DUMB)` | Auto-created on open (V1) | Low |
| **FB Map** | `mmap()` with offset | `mmap()` at offset 0 | Low |
| **FB Present** | `ioctl(DRM_IOCTL_MODE_SETCRTC)` + damage | `write(damage)` + `fsync()` | Low |
| **Input Open** | `/dev/input/event0` + device scan | `input:consumer` | Low |
| **Input Read** | `read()` → `InputEvent` | `read()` → `orbclient::Event` | Medium |
| **Key Mapping** | Key codes (KEY_LEFT) | Scan codes (0x4B) | Medium |

**Overall Assessment**: Moderate complexity. Display is straightforward with V1 API. Input requires understanding orbclient Event format and scan code mapping.
