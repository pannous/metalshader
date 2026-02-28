// macOS hardware display resolution management via CoreGraphics
#![cfg(target_os = "macos")]

use std::ffi::c_void;

type CGDirectDisplayID = u32;
type CGDisplayModeRef = *mut c_void;
type CGError = i32;
type CFArrayRef = *mut c_void;
type CFIndex = isize;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGMainDisplayID() -> CGDirectDisplayID;
    fn CGDisplayCopyDisplayMode(display: CGDirectDisplayID) -> CGDisplayModeRef;
    fn CGDisplayCopyAllDisplayModes(display: CGDirectDisplayID, options: *mut c_void) -> CFArrayRef;
    fn CGDisplaySetDisplayMode(display: CGDirectDisplayID, mode: CGDisplayModeRef, options: *mut c_void) -> CGError;
    fn CGDisplayModeGetWidth(mode: CGDisplayModeRef) -> usize;
    fn CGDisplayModeGetHeight(mode: CGDisplayModeRef) -> usize;
    fn CGDisplayModeGetRefreshRate(mode: CGDisplayModeRef) -> f64;
    fn CGDisplayModeRelease(mode: CGDisplayModeRef);
    fn CGDisplayModeRetain(mode: CGDisplayModeRef);
    fn CGDisplayModeIsUsableForDesktopGUI(mode: CGDisplayModeRef) -> bool;
    fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(array: CFArrayRef, idx: CFIndex) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

// Raw pointer wrapper — only used on main thread
struct ModeRef(CGDisplayModeRef);
unsafe impl Send for ModeRef {}
unsafe impl Sync for ModeRef {}

pub struct DisplayMode {
    pub width: usize,
    pub height: usize,
    pub refresh_rate: f64,
    mode_ref: ModeRef,
}

pub struct ResolutionManager {
    display: CGDirectDisplayID,
    original_mode: ModeRef,
    pub modes: Vec<DisplayMode>,
    current_index: Option<usize>,
}

impl ResolutionManager {
    pub fn new() -> Self {
        unsafe {
            let display = CGMainDisplayID();
            let original = CGDisplayCopyDisplayMode(display);
            let all = CGDisplayCopyAllDisplayModes(display, std::ptr::null_mut());
            let count = CFArrayGetCount(all) as usize;

            let mut modes: Vec<DisplayMode> = (0..count)
                .filter_map(|i| {
                    let m = CFArrayGetValueAtIndex(all, i as isize) as CGDisplayModeRef;
                    if !CGDisplayModeIsUsableForDesktopGUI(m) {
                        return None;
                    }
                    let w = CGDisplayModeGetWidth(m);
                    let h = CGDisplayModeGetHeight(m);
                    let r = CGDisplayModeGetRefreshRate(m);
                    CGDisplayModeRetain(m);
                    Some(DisplayMode { width: w, height: h, refresh_rate: r, mode_ref: ModeRef(m) })
                })
                .collect();

            CFRelease(all as *const c_void);

            // Sort by pixel count ascending, then refresh rate descending
            modes.sort_by(|a, b| {
                (a.width * a.height).cmp(&(b.width * b.height))
                    .then_with(|| b.refresh_rate.partial_cmp(&a.refresh_rate)
                        .unwrap_or(std::cmp::Ordering::Equal))
            });
            // One entry per resolution (keep highest refresh rate)
            modes.dedup_by(|a, b| a.width == b.width && a.height == b.height);

            println!("Available display modes ({}):", modes.len());
            for (i, m) in modes.iter().enumerate() {
                println!("  [{}] {}x{} @ {:.0}Hz", i + 1, m.width, m.height, m.refresh_rate);
            }

            Self { display, original_mode: ModeRef(original), modes, current_index: None }
        }
    }

    /// Set display mode by 1-based key (1=lowest, 5=highest, spread evenly)
    pub fn set_by_key(&mut self, key: u8) -> Result<(usize, usize), String> {
        let n = self.modes.len();
        if n == 0 {
            return Err("No display modes available".into());
        }
        // Map key 1..=5 to evenly spread indices across available modes
        let idx = ((key as usize - 1) * (n - 1)) / 4;
        let idx = idx.min(n - 1);
        self.set_index(idx)
    }

    fn set_index(&mut self, idx: usize) -> Result<(usize, usize), String> {
        let (w, h, r, mode_ptr) = {
            let m = &self.modes[idx];
            (m.width, m.height, m.refresh_rate, m.mode_ref.0)
        };
        unsafe {
            let err = CGDisplaySetDisplayMode(self.display, mode_ptr, std::ptr::null_mut());
            if err != 0 {
                return Err(format!("CGDisplaySetDisplayMode error: {}", err));
            }
        }
        self.current_index = Some(idx);
        println!("Display -> {}x{} @ {:.0}Hz", w, h, r);
        Ok((w, h))
    }

    pub fn restore(&self) {
        unsafe {
            CGDisplaySetDisplayMode(self.display, self.original_mode.0, std::ptr::null_mut());
        }
        println!("Display resolution restored");
    }
}

impl Drop for ResolutionManager {
    fn drop(&mut self) {
        // Only restore if we actually changed the resolution
        if self.current_index.is_some() {
            self.restore();
        }
        unsafe {
            CGDisplayModeRelease(self.original_mode.0);
            for m in &self.modes {
                CGDisplayModeRelease(m.mode_ref.0);
            }
        }
    }
}
