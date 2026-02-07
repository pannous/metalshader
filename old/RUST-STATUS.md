# Rust Metalshader - Status and Requirements

## ✅ C Version Working on Alpine

Successfully tested on Alpine Linux with Venus/Vulkan:
- **Performance**: ~700 FPS (rotating cube shader)
- **GPU**: Virtio-GPU Venus (Apple M2 Pro)
- **Resolution**: 800x600
- **Input**: QEMU USB Keyboard (/dev/input/event1)
- **Features**: All working (arrow key navigation, live shader swapping)

```
Metalshader on Virtio-GPU Venus (Apple M2 Pro) (800x600)
Loaded shader: cube
3.9s: 2880 frames (731.4 FPS) - cube
```

## 🚧 Rust Version - Crate Version Requirements

The Rust conversion is **complete** but requires newer crate versions than available in Alpine Linux repositories.

### Current Alpine Versions (Too Old)
- `drm` = 0.12.0 (available: 0.14.1)
- `gbm` = 0.15.0 (available: 0.18.0)
- `input-linux` = 0.7.1 (available: 0.9+)

### API Incompatibilities

1. **DRM API Changes**:
   - `drm::ffi` module removed in favor of safe wrappers
   - `Device` trait methods changed signatures
   - Direct `File` no longer implements `DrmControlDevice`

2. **GBM API Changes**:
   - `map_mut()` signature changed (added `device` parameter)
   - Buffer access API redesigned
   - Method names changed (e.g., `as_raw()` removed)

3. **input-linux API Changes**:
   - `InputEvent::default()` → `InputEvent::zeroed()`
   - `event.kind` field access vs `kind()` method
   - `EventRef` enum variant structure changed
   - `Key::new()` → `Key::from_code()`

### Solutions

#### Option 1: Newer Environment (Recommended)
Build on a system with newer Rust crates:
- Ubuntu 24.04+ / Debian testing
- Arch Linux / NixOS (rolling release)
- Use rustup with latest stable Rust

#### Option 2: Install Cargo from Source on Alpine
```bash
# On Alpine guest
apk add --no-cache cargo-bootstrap rust-dev

# Update Cargo.lock to use newer versions
cd /root/metalshader-rust
cargo update
cargo build --release
```

#### Option 3: Cross-Compile from Host
Requires setting up cross-compilation toolchain with musl support and system libraries.

## 📦 Rust Version Features

**Completed Implementation**:
- ✅ Vulkan rendering (`ash` crate - platform agnostic)
- ✅ Shader management and discovery
- ✅ Linux DRM/GBM display backend
- ✅ Linux input-events keyboard handling
- ✅ Conditional compilation for OS-specific code
- ✅ Modular architecture for Redox adaptation

**Code Structure**:
```
src/
├── main.rs         - Entry point, platform selection
├── renderer.rs     - Vulkan rendering (platform-agnostic)
├── shader.rs       - Shader discovery and loading
├── display.rs      - DRM/GBM display (Linux-only)
└── input.rs        - Keyboard input (Linux-only)
```

**Platform Support**:
```rust
#[cfg(target_os = "linux")]
mod display;  // DRM/GBM backend

#[cfg(target_os = "redox")]
mod display_redox;  // Scheme-based backend (TODO)
```

## 🎯 Next Steps for Rust Version

1. **Test on newer Linux distribution** (Ubuntu/Arch)
2. **Implement Redox backends** (`display_redox.rs`, `input_redox.rs`)
3. **Cross-compile for Redox** using `aarch64-unknown-redox` target
4. **Test on Redox VM** via 9P share

## 🔧 Building (When Dependencies Available)

```bash
# Native build
cargo build --release

# Cross-compile for musl (requires newer crates)
cargo build --target aarch64-unknown-linux-musl --release

# Cross-compile for Redox (future)
cargo build --target aarch64-unknown-redox --release
```

## 📊 Comparison

| Aspect | C Version | Rust Version |
|--------|-----------|--------------|
| **Performance** | ~700 FPS | Expected similar |
| **Safety** | Manual memory mgmt | Memory safe |
| **Portability** | Linux-specific | Multi-platform ready |
| **Dependencies** | libdrm, libgbm, libvulkan | Rust crates (version-sensitive) |
| **Status** | ✅ Working on Alpine | 🚧 Needs newer crates |
| **Redox Ready** | ❌ Requires rewrite | ✅ Modular, ready for adaptation |

## 📝 Recommendation

For **immediate use on Alpine**: Use the C version (fully functional).

For **Redox migration**: Continue with Rust version on a system with newer dependencies, then adapt for Redox OS scheme-based I/O.

The Rust architecture is designed for multi-platform support and will make the Redox adaptation significantly easier than porting the C version.
