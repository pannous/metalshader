// Keyboard input handling via Linux input events
#![cfg(target_os = "linux")]

use input_linux::{EventKind, InputEvent, Key, GenericEvent};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

pub enum KeyEvent {
    Left,
    Right,
    Fullscreen,
    Quit,
    Resolution(u8),  // 1-9 for mode selection
}

pub struct KeyboardInput {
    device: Option<File>,
}

impl KeyboardInput {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
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

    pub fn poll_event(&mut self) -> Option<KeyEvent> {
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

fn read_input_event(file: &mut File, event: &mut InputEvent) -> io::Result<bool> {
    use std::io::Read;

    let event_bytes = unsafe {
        std::slice::from_raw_parts_mut(
            event as *mut _ as *mut u8,
            std::mem::size_of::<InputEvent>(),
        )
    };

    match file.read_exact(event_bytes) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(false),
        Err(e) => Err(e),
    }
}
