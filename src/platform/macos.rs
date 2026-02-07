// macOS platform implementation using winit for windowing and input
#![cfg(target_os = "macos")]

use crate::platform::{DisplayBackend, InputBackend, KeyEvent};
use std::error::Error;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

// ============================================================================
// Display Backend - Winit Window
// ============================================================================

/// Shared state between Display and Input
struct SharedState {
    pending_events: VecDeque<KeyEvent>,
}

pub struct MacOSDisplay {
    width: u32,
    height: u32,
}

impl DisplayBackend for MacOSDisplay {
    fn new() -> Result<Self, Box<dyn Error>> {
        // Note: Running in headless mode for now
        // Full windowed support requires integrating winit's event loop into main.rs
        // See /opt/3d/metalshade/metalshade.cpp for reference implementation with GLFW

        println!("macOS display initialized (headless mode)");
        println!("Note: Rendering without window - output is not displayed");
        println!("To add windowed support, see metalshade.cpp for reference");

        Ok(Self {
            width: 1280,
            height: 800,
        })
    }

    fn get_resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn set_mode(&mut self, mode_number: u8) -> Result<(u32, u32), Box<dyn Error>> {
        // Predefined resolution modes
        let resolutions = [
            (1280, 800),
            (1920, 1080),
            (2560, 1440),
            (3840, 2160),
            (1024, 768),
            (1366, 768),
            (1600, 900),
            (2048, 1152),
            (3440, 1440),
        ];

        let mode_idx = (mode_number - 1) as usize;
        if mode_idx >= resolutions.len() {
            return Err(format!(
                "Mode {} not available (only {} modes)",
                mode_number,
                resolutions.len()
            )
            .into());
        }

        let (width, height) = resolutions[mode_idx];
        self.width = width;
        self.height = height;

        println!("Resolution changed to {}x{}", self.width, self.height);
        Ok((self.width, self.height))
    }

    fn present(&mut self, _data: &[u8], _row_pitch: usize) -> Result<(), Box<dyn Error>> {
        // For now, just verify data is present
        // Full windowed rendering would require swapchain integration
        // Rendering happens in memory only (headless mode)
        Ok(())
    }
}

// ============================================================================
// Input Backend - Keyboard polling
// ============================================================================

pub struct MacOSInput {
    state: Arc<Mutex<SharedState>>,
}

impl InputBackend for MacOSInput {
    fn new() -> Result<Self, Box<dyn Error>> {
        println!("macOS keyboard input initialized");
        Ok(Self {
            state: Arc::new(Mutex::new(SharedState {
                pending_events: VecDeque::new(),
            })),
        })
    }

    fn poll_event(&mut self) -> Option<KeyEvent> {
        if let Ok(mut state) = self.state.lock() {
            state.pending_events.pop_front()
        } else {
            None
        }
    }
}

#[allow(dead_code)]
fn map_key_code(key: &PhysicalKey) -> Option<KeyEvent> {
    match key {
        PhysicalKey::Code(KeyCode::ArrowLeft) => Some(KeyEvent::Left),
        PhysicalKey::Code(KeyCode::ArrowRight) => Some(KeyEvent::Right),
        PhysicalKey::Code(KeyCode::KeyF) => Some(KeyEvent::Fullscreen),
        PhysicalKey::Code(KeyCode::Escape) | PhysicalKey::Code(KeyCode::KeyQ) => {
            Some(KeyEvent::Quit)
        }
        PhysicalKey::Code(KeyCode::Digit1) => Some(KeyEvent::Resolution(1)),
        PhysicalKey::Code(KeyCode::Digit2) => Some(KeyEvent::Resolution(2)),
        PhysicalKey::Code(KeyCode::Digit3) => Some(KeyEvent::Resolution(3)),
        PhysicalKey::Code(KeyCode::Digit4) => Some(KeyEvent::Resolution(4)),
        PhysicalKey::Code(KeyCode::Digit5) => Some(KeyEvent::Resolution(5)),
        PhysicalKey::Code(KeyCode::Digit6) => Some(KeyEvent::Resolution(6)),
        PhysicalKey::Code(KeyCode::Digit7) => Some(KeyEvent::Resolution(7)),
        PhysicalKey::Code(KeyCode::Digit8) => Some(KeyEvent::Resolution(8)),
        PhysicalKey::Code(KeyCode::Digit9) => Some(KeyEvent::Resolution(9)),
        _ => None,
    }
}

// Helper to create standalone window for testing (not used in headless mode)
#[allow(dead_code)]
pub fn create_window() -> Result<(EventLoop<()>, Arc<Window>), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;

    let window_attributes = Window::default_attributes()
        .with_title("Metalshader - Vulkan Shader Viewer")
        .with_inner_size(PhysicalSize::new(1280, 800))
        .with_resizable(true);

    let window = Arc::new(event_loop.create_window(window_attributes)?);

    println!("Created macOS window: {}x{}", 1280, 800);
    Ok((event_loop, window))
}
