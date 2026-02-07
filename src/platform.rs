// Platform abstraction layer for display and input
//
// This module provides traits that abstract over platform-specific
// display and input handling. The goal is to keep the Vulkan renderer
// completely platform-agnostic while allowing different backends for
// display presentation and keyboard input.

use std::error::Error;

/// Platform-agnostic display backend trait
///
/// Implementations handle:
/// - Opening and configuring the display
/// - Querying and setting display resolution
/// - Presenting rendered frames from Vulkan
pub trait DisplayBackend {
    /// Create and initialize the display backend
    fn new() -> Result<Self, Box<dyn Error>>
    where
        Self: Sized;

    /// Get the current display resolution
    ///
    /// Returns (width, height) in pixels
    fn get_resolution(&self) -> (u32, u32);

    /// Set a new display mode/resolution
    ///
    /// `mode` is a mode number (1-9) that maps to predefined resolutions
    /// Returns the new (width, height) after mode change
    fn set_mode(&mut self, mode: u8) -> Result<(u32, u32), Box<dyn Error>>;

    /// Present a rendered frame to the display
    ///
    /// `data` contains the pixel data in BGRA format
    /// `row_pitch` is the number of bytes per row (may differ from width * 4 due to alignment)
    fn present(&mut self, data: &[u8], row_pitch: usize) -> Result<(), Box<dyn Error>>;
}

/// Platform-agnostic input backend trait
///
/// Implementations handle:
/// - Opening and configuring keyboard input
/// - Polling for keyboard events in a non-blocking manner
pub trait InputBackend {
    /// Create and initialize the input backend
    fn new() -> Result<Self, Box<dyn Error>>
    where
        Self: Sized;

    /// Poll for the next keyboard event
    ///
    /// Returns Some(KeyEvent) if an event is available, None otherwise
    /// This function should not block - it returns immediately
    fn poll_event(&mut self) -> Option<KeyEvent>;
}

/// Platform-independent keyboard event types
///
/// These events represent the logical actions the user wants to perform,
/// abstracted from platform-specific scan codes or key codes
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    /// Navigate to previous shader
    Left,
    /// Navigate to next shader
    Right,
    /// Toggle fullscreen mode
    Fullscreen,
    /// Quit the application
    Quit,
    /// Switch to a specific resolution mode (1-9)
    Resolution(u8),
}

// Platform-specific implementations
#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "redox")]
pub mod redox;

#[cfg(target_os = "macos")]
pub mod macos;
