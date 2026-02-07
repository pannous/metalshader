// Linux platform implementation using DRM/KMS and evdev
#![cfg(target_os = "linux")]

use crate::platform::{DisplayBackend, InputBackend, KeyEvent};
use std::error::Error;

// ============================================================================
// Display Backend - DRM/KMS
// ============================================================================

use drm::control::{connector, crtc, framebuffer, Device as ControlDevice, dumbbuffer::DumbBuffer};
use drm::buffer::{Buffer, DrmFourcc};
use drm::Device;
use std::fs::{File, OpenOptions};
use std::os::unix::io::{AsFd, AsRawFd, BorrowedFd, RawFd};

/// Wrapper for DRM device that implements required traits
#[derive(Debug)]
struct DrmCard(File);

impl AsFd for DrmCard {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Device for DrmCard {}
impl ControlDevice for DrmCard {}

pub struct LinuxDisplay {
    drm_fd: RawFd,
    drm_card: DrmCard,
    dumb_buffer: DumbBuffer,
    fb_id: framebuffer::Handle,
    crtc_id: crtc::Handle,
    connector_handle: connector::Handle,
    modes: Vec<drm::control::Mode>,
    current_mode_idx: usize,
    width: u32,
    height: u32,
}

impl DisplayBackend for LinuxDisplay {
    fn new() -> Result<Self, Box<dyn Error>> {
        // Open DRM device
        let drm_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/dri/card0")
            .map_err(|e| format!("Failed to open /dev/dri/card0: {}", e))?;

        let drm_fd = drm_file.as_raw_fd();
        let drm_card = DrmCard(drm_file);

        // Get resources
        let res = drm_card.resource_handles()
            .map_err(|e| format!("Failed to get DRM resources: {}", e))?;

        // Find connected connector
        let connector_handle = res
            .connectors()
            .iter()
            .find_map(|&conn_handle| {
                let conn = drm_card.get_connector(conn_handle, true).ok()?;
                if conn.state() == connector::State::Connected {
                    Some(conn_handle)
                } else {
                    None
                }
            })
            .ok_or("No connected display found")?;

        let connector = drm_card.get_connector(connector_handle, true)?;

        // Get all available modes
        let modes: Vec<_> = connector.modes().to_vec();
        eprintln!("Available modes: {} total", modes.len());
        for (i, m) in modes.iter().take(9).enumerate() {
            eprintln!("  [{}] {}x{}", i + 1, m.size().0, m.size().1);
        }

        let current_mode_idx = 0;
        let mode = modes.first()
            .ok_or("No display mode available")?;

        let (width, height) = mode.size();
        eprintln!("Selected mode: [1] {}x{}", width, height);

        // Get encoder and CRTC
        let crtc_id = connector
            .current_encoder()
            .and_then(|enc_handle| drm_card.get_encoder(enc_handle).ok())
            .and_then(|enc| enc.crtc())
            .or_else(|| res.crtcs().first().copied())
            .ok_or("No CRTC found")?;

        eprintln!("Creating dumb buffer: {}x{}", width, height);
        // Create DumbBuffer (CPU-accessible buffer for virtio-gpu)
        let dumb_buffer = drm_card.create_dumb_buffer(
            (width as u32, height as u32),
            DrmFourcc::Xrgb8888,
            32 // bpp
        ).map_err(|e| format!("Failed to create dumb buffer {}x{}: {}", width, height, e))?;

        eprintln!("Creating framebuffer");
        // Create framebuffer
        let fb_id = drm_card.add_framebuffer(&dumb_buffer, 24, 32)
            .map_err(|e| format!("Failed to add framebuffer: {}", e))?;

        eprintln!("Setting CRTC");
        // Set CRTC
        drm_card.set_crtc(
            crtc_id,
            Some(fb_id),
            (0, 0),
            &[connector_handle],
            Some(*mode),
        ).map_err(|e| format!("Failed to set CRTC: {}", e))?;

        Ok(Self {
            drm_fd,
            drm_card,
            dumb_buffer,
            fb_id,
            crtc_id,
            connector_handle,
            modes,
            current_mode_idx,
            width: width as u32,
            height: height as u32,
        })
    }

    fn get_resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn set_mode(&mut self, mode_number: u8) -> Result<(u32, u32), Box<dyn Error>> {
        let mode_idx = (mode_number - 1) as usize;
        if mode_idx >= self.modes.len() {
            return Err(format!("Mode {} not available (only {} modes)", mode_number, self.modes.len()).into());
        }

        let mode = &self.modes[mode_idx];
        let (width, height) = mode.size();

        eprintln!("\nSwitching to mode [{}]: {}x{}", mode_number, width, height);

        // Remove old framebuffer
        let _ = self.drm_card.destroy_framebuffer(self.fb_id);

        // Destroy old dumb buffer
        let _ = self.drm_card.destroy_dumb_buffer(self.dumb_buffer);

        // Create new dumb buffer at new resolution
        self.dumb_buffer = self.drm_card.create_dumb_buffer(
            (width as u32, height as u32),
            DrmFourcc::Xrgb8888,
            32
        )?;

        // Create new framebuffer
        self.fb_id = self.drm_card.add_framebuffer(&self.dumb_buffer, 24, 32)?;

        // Set CRTC to new mode
        self.drm_card.set_crtc(
            self.crtc_id,
            Some(self.fb_id),
            (0, 0),
            &[self.connector_handle],
            Some(*mode),
        )?;

        self.current_mode_idx = mode_idx;
        self.width = width as u32;
        self.height = height as u32;

        Ok((self.width, self.height))
    }

    fn present(&mut self, frame_data: &[u8], src_row_pitch: usize) -> Result<(), Box<dyn Error>> {
        let bytes_per_pixel = 4;
        let row_size = self.width as usize * bytes_per_pixel;
        let dst_stride = self.dumb_buffer.pitch() as usize;

        // Map DumbBuffer for CPU access
        let mut mapping = self.drm_card.map_dumb_buffer(&mut self.dumb_buffer)?;
        let buffer_slice = mapping.as_mut();

        static mut DEBUG_COUNT: u32 = 0;
        unsafe {
            if DEBUG_COUNT == 0 {
                eprintln!("=== DISPLAY DEBUG ===");
                eprintln!("Frame data len: {}, src_row_pitch: {}", frame_data.len(), src_row_pitch);
                eprintln!("Buffer len: {}, dst_stride: {}", buffer_slice.len(), dst_stride);
                eprintln!("Dimensions: {}x{}, row_size: {}", self.width, self.height, row_size);
                eprintln!("First 16 bytes of source: {:02x?}", &frame_data[0..16.min(frame_data.len())]);
            }
        }

        for y in 0..self.height as usize {
            let dst_offset = y * dst_stride;
            let src_offset = y * src_row_pitch;  // Use Vulkan's row pitch
            let copy_len = row_size;
            if dst_offset + copy_len <= buffer_slice.len() && src_offset + copy_len <= frame_data.len() {
                buffer_slice[dst_offset..dst_offset + copy_len]
                    .copy_from_slice(&frame_data[src_offset..src_offset + copy_len]);
            }
        }

        unsafe {
            if DEBUG_COUNT == 0 {
                eprintln!("First 16 bytes of dest after copy: {:02x?}", &buffer_slice[0..16.min(buffer_slice.len())]);
                DEBUG_COUNT = 1;
            }
        }

        // CRITICAL: Mark framebuffer as dirty so DRM actually displays it!
        drop(mapping);  // Unmap before dirty call
        use drm::control::ClipRect;
        let clip = ClipRect::new(0, 0, self.width as u16, self.height as u16);
        self.drm_card.dirty_framebuffer(self.fb_id, &[clip])?;

        Ok(())
    }
}

// ============================================================================
// Input Backend - evdev
// ============================================================================

use input_linux::{EventKind, InputEvent, Key, GenericEvent};

pub struct LinuxInput {
    device: Option<File>,
}

impl InputBackend for LinuxInput {
    fn new() -> Result<Self, Box<dyn Error>> {
        // Try to find a keyboard device
        eprintln!("Scanning for keyboard input devices...");
        for i in 0..10 {
            let path = format!("/dev/input/event{}", i);
            if let Ok(file) = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&path)
            {
                // Try to get device name to verify it's a keyboard
                let name = get_device_name(file.as_raw_fd());
                eprintln!("  {}: {}", path, name);
                if name.to_lowercase().contains("keyboard") || name.to_lowercase().contains("input") {
                    println!("Using input: {} ({})", path, name);
                    return Ok(Self { device: Some(file) });
                }
            }
        }

        println!("Warning: No keyboard input found, arrow key navigation disabled");
        Ok(Self { device: None })
    }

    fn poll_event(&mut self) -> Option<KeyEvent> {
        let device = self.device.as_mut()?;

        // Read events in non-blocking mode
        loop {
            let mut event = InputEvent::zeroed();
            match read_input_event(device, &mut event) {
                Ok(true) => {
                    // Check for key press events (value == 1 means press, not release)
                    if event.kind == EventKind::Key && event.value() == 1 {
                        // Check for number keys using raw codes (KEY_1 = 2, KEY_2 = 3, etc.)
                        if event.code >= 2 && event.code <= 10 {
                            let mode_num = if event.code == 10 { 0 } else { event.code - 1 } as u8;
                            if mode_num >= 1 && mode_num <= 9 {
                                return Some(KeyEvent::Resolution(mode_num));
                            }
                        }

                        // Get key code from event for named keys
                        if let Ok(key) = Key::from_code(event.code) {
                            match key {
                                Key::Left => return Some(KeyEvent::Left),
                                Key::Right => return Some(KeyEvent::Right),
                                Key::F => return Some(KeyEvent::Fullscreen),
                                Key::Esc | Key::Q => return Some(KeyEvent::Quit),
                                _ => {}
                            }
                        }
                    }
                }
                Ok(false) => return None, // No more events
                Err(_) => return None,
            }
        }
    }
}

// Helper functions for Linux input

fn get_device_name(fd: i32) -> String {
    // EVIOCGNAME ioctl: _IOC(_IOC_READ, 'E', 0x06, len)
    // Properly construct ioctl number for aarch64
    const _IOC_NRBITS: u32 = 8;
    const _IOC_TYPEBITS: u32 = 8;
    const _IOC_SIZEBITS: u32 = 14;
    const _IOC_NRSHIFT: u32 = 0;
    const _IOC_TYPESHIFT: u32 = _IOC_NRSHIFT + _IOC_NRBITS;
    const _IOC_SIZESHIFT: u32 = _IOC_TYPESHIFT + _IOC_TYPEBITS;
    const _IOC_DIRSHIFT: u32 = _IOC_SIZESHIFT + _IOC_SIZEBITS;
    const _IOC_READ: u32 = 2;

    const EVIOCGNAME_256: u32 = (_IOC_READ << _IOC_DIRSHIFT)
                               | (0x45 << _IOC_TYPESHIFT)  // 'E'
                               | (0x06 << _IOC_NRSHIFT)
                               | (256 << _IOC_SIZESHIFT);

    let mut name = vec![0u8; 256];
    unsafe {
        if libc::ioctl(fd, EVIOCGNAME_256 as libc::c_int, name.as_mut_ptr()) >= 0 {
            let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            String::from_utf8_lossy(&name[..len]).to_string()
        } else {
            "Unknown".to_string()
        }
    }
}

fn read_input_event(file: &mut File, event: &mut InputEvent) -> std::io::Result<bool> {
    use std::io::Read;

    let event_bytes = unsafe {
        std::slice::from_raw_parts_mut(
            event as *mut _ as *mut u8,
            std::mem::size_of::<InputEvent>(),
        )
    };

    match file.read_exact(event_bytes) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
        Err(e) => Err(e),
    }
}
