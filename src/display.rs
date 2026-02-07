// DRM display management using DumbBuffer
#![cfg(target_os = "linux")]

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

pub struct Display {
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

impl Display {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
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

    pub fn set_mode(&mut self, mode_number: u8) -> Result<(u32, u32), Box<dyn std::error::Error>> {
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

    pub fn get_resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn present(&mut self, frame_data: &[u8], src_row_pitch: usize) -> Result<(), Box<dyn std::error::Error>> {
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
