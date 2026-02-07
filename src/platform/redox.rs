// Redox OS platform implementation using schemes
#![cfg(target_os = "redox")]

use crate::platform::{DisplayBackend, InputBackend, KeyEvent};
use std::error::Error;
use std::fs::File;

// ============================================================================
// Display Backend - Redox Graphics Scheme
// ============================================================================

pub struct RedoxDisplay {
    display: File,
    width: u32,
    height: u32,
    fb_ptr: *mut u8,
    fb_size: usize,
}

impl DisplayBackend for RedoxDisplay {
    fn new() -> Result<Self, Box<dyn Error>> {
        // Open the display scheme (V1 API for simplicity)
        // Format: "display.virtio-gpu:<vt>.<screen>"
        // VT 2 is the default, screen 0 is primary display
        let display = File::open("display.virtio-gpu:2.0")
            .map_err(|e| format!("Failed to open display scheme: {}", e))?;

        // Get resolution from fpath
        // The path returned is: "display.virtio-gpu:2.0/width/height"
        use std::os::unix::io::AsRawFd;
        let mut path_buf = vec![0u8; 256];
        let fd = display.as_raw_fd();

        // Use syscall::fpath to get the full path with resolution
        let path_len = unsafe {
            syscall::syscall!(syscall::SYS_FPATH, fd as usize, path_buf.as_mut_ptr(), path_buf.len())
                .map_err(|e| format!("fpath failed: {}", e))?
        };

        let path = std::str::from_utf8(&path_buf[..path_len])
            .map_err(|_| "Invalid UTF-8 in display path")?;

        eprintln!("Display path: {}", path);

        // Parse width and height from path
        // Expected format: "display.virtio-gpu:2.0/width/height"
        let parts: Vec<&str> = path.split('/').collect();
        let (width, height) = if parts.len() >= 3 {
            let w = parts[parts.len() - 2].parse::<u32>()
                .map_err(|_| format!("Failed to parse width from path: {}", path))?;
            let h = parts[parts.len() - 1].parse::<u32>()
                .map_err(|_| format!("Failed to parse height from path: {}", path))?;
            (w, h)
        } else {
            // Fallback to default resolution
            eprintln!("Warning: Could not parse resolution from path '{}', using default 1920x1080", path);
            (1920, 1080)
        };

        eprintln!("Display resolution: {}x{}", width, height);

        // Map the framebuffer using mmap
        let fb_size = (width * height * 4) as usize;
        let fb_ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                fb_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };

        if fb_ptr == libc::MAP_FAILED {
            return Err(format!("mmap failed: {}", std::io::Error::last_os_error()).into());
        }

        eprintln!("Framebuffer mapped at {:?}, size {}", fb_ptr, fb_size);

        Ok(Self {
            display,
            width,
            height,
            fb_ptr: fb_ptr as *mut u8,
            fb_size,
        })
    }

    fn get_resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn set_mode(&mut self, mode_number: u8) -> Result<(u32, u32), Box<dyn Error>> {
        // Resolution switching on Redox V1 API is not directly supported
        // Would need to close and reopen with different path, or use V2 API
        Err(format!("Resolution switching not implemented for Redox yet (requested mode {})", mode_number).into())
    }

    fn present(&mut self, data: &[u8], row_pitch: usize) -> Result<(), Box<dyn Error>> {
        // Copy frame data to mapped framebuffer
        let bytes_per_pixel = 4;
        let row_size = self.width as usize * bytes_per_pixel;

        unsafe {
            let fb = std::slice::from_raw_parts_mut(self.fb_ptr, self.fb_size);

            // Handle row pitch differences
            for y in 0..self.height as usize {
                let dst_offset = y * row_size;
                let src_offset = y * row_pitch;

                if dst_offset + row_size <= fb.len() && src_offset + row_size <= data.len() {
                    fb[dst_offset..dst_offset + row_size]
                        .copy_from_slice(&data[src_offset..src_offset + row_size]);
                }
            }
        }

        // Write damage region to trigger update
        use std::io::Write;

        // Damage struct from graphics_ipc::v2::Damage
        #[repr(C)]
        struct Damage {
            x: u32,
            y: u32,
            width: u32,
            height: u32,
        }

        let damage = Damage {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        };

        let damage_bytes = unsafe {
            std::slice::from_raw_parts(
                &damage as *const _ as *const u8,
                std::mem::size_of::<Damage>(),
            )
        };

        // Write damage to scheme
        use std::io::Write as _;
        self.display.write_all(damage_bytes)
            .map_err(|e| format!("Failed to write damage: {}", e))?;

        // Flush (fsync triggers update_plane)
        self.display.sync_all()
            .map_err(|e| format!("Failed to sync display: {}", e))?;

        Ok(())
    }
}

impl Drop for RedoxDisplay {
    fn drop(&mut self) {
        // Unmap the framebuffer
        unsafe {
            libc::munmap(self.fb_ptr as *mut libc::c_void, self.fb_size);
        }
    }
}

// ============================================================================
// Input Backend - Redox Input Scheme
// ============================================================================

pub struct RedoxInput {
    file: File,
}

impl InputBackend for RedoxInput {
    fn new() -> Result<Self, Box<dyn Error>> {
        // Open the input consumer scheme
        let file = File::open("input:consumer")
            .map_err(|e| format!("Failed to open input scheme: {}", e))?;

        // Set non-blocking mode
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();

        unsafe {
            syscall::syscall!(
                syscall::SYS_FCNTL,
                fd as usize,
                syscall::F_SETFL,
                syscall::O_NONBLOCK
            ).map_err(|e| format!("fcntl failed: {}", e))?;
        }

        eprintln!("Input device opened: input:consumer");

        Ok(Self { file })
    }

    fn poll_event(&mut self) -> Option<KeyEvent> {
        use orbclient::{Event, EventOption};
        use std::io::Read;

        // Read events from input scheme
        let mut events = [Event::default(); 16];
        let event_bytes = unsafe {
            std::slice::from_raw_parts_mut(
                events.as_mut_ptr() as *mut u8,
                events.len() * std::mem::size_of::<Event>(),
            )
        };

        let bytes_read = match self.file.read(event_bytes) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return None,
            Err(_) => return None,
        };

        let count = bytes_read / std::mem::size_of::<Event>();

        // Process events
        for event in &events[..count] {
            if let EventOption::Key(key_event) = event.to_option() {
                // Only process key press events
                if !key_event.pressed {
                    continue;
                }

                // Map PS/2 scan codes to KeyEvent
                // Scan codes based on PS/2 Set 1
                match key_event.scancode {
                    0x4B => return Some(KeyEvent::Left),       // Left arrow
                    0x4D => return Some(KeyEvent::Right),      // Right arrow
                    0x21 => return Some(KeyEvent::Fullscreen), // F key
                    0x01 => return Some(KeyEvent::Quit),       // ESC
                    0x10 => return Some(KeyEvent::Quit),       // Q key
                    0x02..=0x0A => {
                        // Number keys 1-9
                        // 0x02 = '1', 0x03 = '2', ..., 0x0A = '9'
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
