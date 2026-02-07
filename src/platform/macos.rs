// macOS platform implementation using winit for windowing and input
#![cfg(target_os = "macos")]

use crate::platform::{DisplayBackend, InputBackend, KeyEvent};
use std::error::Error;
use std::sync::{Arc, Mutex};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

// ============================================================================
// Display Backend - Winit Window
// ============================================================================

pub struct MacOSDisplay {
    window: Arc<Window>,
    event_loop: Option<EventLoop<()>>,
    width: u32,
    height: u32,
    pending_events: Arc<Mutex<Vec<KeyEvent>>>,
}

impl DisplayBackend for MacOSDisplay {
    fn new() -> Result<Self, Box<dyn Error>> {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);

        let window_attributes = Window::default_attributes()
            .with_title("Metalshader - Vulkan Shader Viewer")
            .with_inner_size(winit::dpi::PhysicalSize::new(1280, 800))
            .with_resizable(true);

        let window = Arc::new(event_loop.create_window(window_attributes)?);
        let size = window.inner_size();

        println!("Created macOS window: {}x{}", size.width, size.height);

        Ok(Self {
            window,
            event_loop: Some(event_loop),
            width: size.width,
            height: size.height,
            pending_events: Arc::new(Mutex::new(Vec::new())),
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
        self.window
            .request_inner_size(winit::dpi::PhysicalSize::new(width, height));

        // Get actual size after resize
        let actual_size = self.window.inner_size();
        self.width = actual_size.width;
        self.height = actual_size.height;

        println!("Resolution changed to {}x{}", self.width, self.height);
        Ok((self.width, self.height))
    }

    fn present(&mut self, _data: &[u8], _row_pitch: usize) -> Result<(), Box<dyn Error>> {
        // On macOS with Vulkan/MoltenVK, presentation is handled by the Vulkan swapchain
        // We just need to request a redraw to keep the event loop running
        self.window.request_redraw();
        Ok(())
    }
}

impl MacOSDisplay {
    pub fn get_window(&self) -> &Arc<Window> {
        &self.window
    }

    pub fn process_events(&mut self) -> Result<(), Box<dyn Error>> {
        let pending = self.pending_events.clone();
        let window_arc = self.window.clone();

        if let Some(event_loop) = self.event_loop.take() {
            let mut handler = MacOSEventHandler {
                window: window_arc,
                pending_events: pending,
            };

            event_loop.run_app(&mut handler)?;
        }

        Ok(())
    }
}

struct MacOSEventHandler {
    window: Arc<Window>,
    pending_events: Arc<Mutex<Vec<KeyEvent>>>,
}

impl ApplicationHandler for MacOSEventHandler {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if let Some(key_event) = map_key_code(&event.physical_key) {
                        if let Ok(mut pending) = self.pending_events.lock() {
                            pending.push(key_event);
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Rendering is handled by the main loop
            }
            _ => {}
        }
    }
}

// ============================================================================
// Input Backend - Winit Events
// ============================================================================

pub struct MacOSInput {
    pending_events: Arc<Mutex<Vec<KeyEvent>>>,
}

impl InputBackend for MacOSInput {
    fn new() -> Result<Self, Box<dyn Error>> {
        println!("macOS keyboard input initialized");
        Ok(Self {
            pending_events: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn poll_event(&mut self) -> Option<KeyEvent> {
        if let Ok(mut pending) = self.pending_events.lock() {
            if !pending.is_empty() {
                return Some(pending.remove(0));
            }
        }
        None
    }
}

impl MacOSInput {
    pub fn set_event_queue(&mut self, queue: Arc<Mutex<Vec<KeyEvent>>>) {
        self.pending_events = queue;
    }
}

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
