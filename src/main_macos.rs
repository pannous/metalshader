// macOS-specific main with windowed swapchain support
#![cfg(target_os = "macos")]

use std::sync::{Arc, Mutex};
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use objc2::runtime::{AnyObject, AnyClass};
use objc2::sel;

use crate::macos_resolution::ResolutionManager;
use crate::renderer_swapchain::SwapchainRenderer;
use crate::shader::ShaderManager;
use crate::shader_compiler::ShaderCompiler;

// Pending file path from Finder "Open With" → shader switcher
static PENDING_FILE: Mutex<Option<String>> = Mutex::new(None);

fn store_pending_path_str(path: String) {
    if let Ok(mut guard) = PENDING_FILE.lock() {
        *guard = Some(path);
    }
}

/// application:openFile: called by AppKit for both initial launch-with-file AND
/// "Open With" while app is running. Must be added to WinitApplicationDelegate.
extern "C" fn app_open_file(_self: *mut AnyObject, _sel: objc2::runtime::Sel,
    _app: *mut AnyObject, filename: *mut AnyObject) -> bool
{
    if filename.is_null() { return false; }
    let utf8: *const std::ffi::c_char = unsafe { objc2::msg_send![filename, UTF8String] };
    if utf8.is_null() { return false; }
    let s = unsafe { std::ffi::CStr::from_ptr(utf8) }.to_string_lossy().into_owned();
    store_pending_path_str(s);
    true
}

/// Inject application:openFile: into WinitApplicationDelegate BEFORE EventLoop::new()
/// so it's present when applicationWillFinishLaunching fires.
fn inject_open_file_handler() {
    unsafe {
        // The class name is registered by winit's declare_class! macro
        let cls = AnyClass::get("WinitApplicationDelegate");
        let cls = match cls {
            Some(c) => c as *const AnyClass as *mut objc2::ffi::objc_class,
            None => {
                // Class not registered yet - we're too early; it will be added by EventLoop::new()
                // We'll re-try after EventLoop::new() in run_macos()
                eprintln!("[openFile] WinitApplicationDelegate not found yet");
                return;
            }
        };
        let sel = sel!(application:openFile:);
        // types: "B@:@@" = BOOL return, id self, SEL, id NSApplication, id NSString
        let _added = objc2::ffi::class_addMethod(
            cls,
            sel.as_ptr() as *const _,
            Some(std::mem::transmute::<extern "C" fn(*mut AnyObject, objc2::runtime::Sel, *mut AnyObject, *mut AnyObject) -> bool,
                unsafe extern "C" fn()>(app_open_file)),
            b"B@:@@\0".as_ptr() as *const _,
        );
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct ShaderToyUBO {
    i_resolution: [f32; 3],
    i_time: f32,
    i_mouse: [f32; 4],
    i_scroll: [f32; 2],  // Accumulated scroll offset (x, y) for zoom
    i_button_left: f32,   // Button press duration in seconds
    i_button_right: f32,
    i_button_middle: f32,
    i_button_4: f32,
    i_button_5: f32,
    i_pan: [f32; 2],     // Accumulated pan offset (x, y) in pixels for drag
}

struct MetalshaderApp {
    window: Option<Arc<Window>>,
    renderer: Option<SwapchainRenderer>,
    shader_manager: ShaderManager,
    #[allow(dead_code)]
    shader_compiler: ShaderCompiler,
    resolution_manager: ResolutionManager,
    current_shader_idx: usize,
    start_time: Instant,
    frame_count: u32,
    reload_requested: bool,
    // Mouse and scroll state
    mouse_x: f64,
    mouse_y: f64,
    mouse_smooth_x: f64,  // Smoothed mouse position for zoom focal point
    mouse_smooth_y: f64,
    mouse_click_x: f64,
    mouse_click_y: f64,
    mouse_left_pressed: bool,
    mouse_right_pressed: bool,
    mouse_middle_pressed: bool,
    button_press_duration: [f32; 5],  // Duration in seconds for each button
    scroll_x: f32,
    scroll_y: f32,
    pan_offset_x: f32,     // Pan in pixels (for shader)
    pan_offset_y: f32,
    base_pan_x: f32,       // Pan in complex-plane units (zoom-independent)
    base_pan_y: f32,
    last_frame_time: Instant,
}

impl MetalshaderApp {
    fn resolve_shader_path(path: &str) -> String {
        use std::path::Path;

        // Remove trailing dot if present
        let working_path = path.trim_end_matches('.').to_string();

        // Check if file exists as-is
        if Path::new(&working_path).exists() {
            return working_path;
        }

        // Check if path has NO extension
        let path_obj = Path::new(&working_path);
        let has_extension = path_obj.extension().is_some();

        if !has_extension {
            // Try adding common fragment shader extensions
            for ext in &[".frag", ".fsh", ".glsl"] {
                let test_path = format!("{}{}", working_path, ext);
                if Path::new(&test_path).exists() {
                    println!("✓ Auto-detected extension: {}", test_path);
                    return test_path;
                }
            }
        }

        working_path
    }

    fn new(shader_path: &str) -> Self {
        let mut shader_manager = ShaderManager::new();
        let shader_compiler = ShaderCompiler::new();

        // Resolve shader path with auto-detection
        let resolved_path = Self::resolve_shader_path(shader_path);

        // First, try to compile the requested shader if it's a source file
        if resolved_path.ends_with(".frag") || resolved_path.ends_with(".glsl") {
            match shader_compiler.compile_if_needed(&resolved_path) {
                Ok(_base_name) => {
                    println!("✓ Shader compiled successfully");
                }
                Err(e) => {
                    eprintln!("Warning: Failed to compile shader: {}", e);
                    eprintln!("Make sure glslangValidator is installed: brew install glslang");
                }
            }
        }

        // Build shader search paths: include bundle Resources/shaders if running from a bundle
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let bundle_shaders = exe_dir.as_ref()
            .map(|d| d.join("../Resources/shaders"))
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().into_owned());

        // When running from bundle, use bundle shaders exclusively to avoid duplicates.
        // Fall back to local dirs only when not bundled (dev/debug mode).
        let bundle_str;
        let search_dirs: Vec<&str> = if let Some(ref bs) = bundle_shaders {
            bundle_str = bs.as_str();
            vec![bundle_str]
        } else {
            vec![".", "./shaders", "/root/metalshade/shaders"]
        };

        if let Err(e) = shader_manager.scan_shaders(&search_dirs) {
            eprintln!("Warning: Failed to scan shaders: {}", e);
        }

        if shader_manager.is_empty() {
            eprintln!("No compiled shaders found.");
            eprintln!("Searched: . ./shaders /root/metalshade/shaders + bundle Resources/shaders");
            eprintln!("Compile shaders with: glslangValidator -V <shader>.vert -o <shader>.vert.spv");
        } else {
            shader_manager.print_available();
        }

        let base_shader_path = MetalshaderApp::shader_name_from_path(&resolved_path);

        let current_shader_idx = shader_manager
            .find_by_name(&base_shader_path)
            .unwrap_or(0);

        println!("Starting with shader: {}",
            shader_manager.get(current_shader_idx)
                .map(|s| s.name.as_str())
                .unwrap_or("(none)"));

        Self {
            window: None,
            renderer: None,
            shader_manager,
            shader_compiler,
            resolution_manager: ResolutionManager::new(),
            current_shader_idx,
            start_time: Instant::now(),
            frame_count: 0,
            reload_requested: true,
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_smooth_x: 0.0,
            mouse_smooth_y: 0.0,
            mouse_click_x: 0.0,
            mouse_click_y: 0.0,
            mouse_left_pressed: false,
            mouse_right_pressed: false,
            mouse_middle_pressed: false,
            button_press_duration: [0.0; 5],
            scroll_x: 0.0,
            scroll_y: 0.0,
            pan_offset_x: 0.0,
            pan_offset_y: 0.0,
            base_pan_x: 0.0,
            base_pan_y: 0.0,
            last_frame_time: Instant::now(),
        }
    }

    fn change_resolution(&mut self, key: u8) {
        let is_fullscreen = self.window.as_ref()
            .map(|w| w.fullscreen().is_some())
            .unwrap_or(false);

        if is_fullscreen {
            // Change actual hardware display resolution
            match self.resolution_manager.set_by_key(key) {
                Ok((w, h)) => println!("\n[{}] Hardware resolution -> {}x{}", key, w, h),
                Err(e) => eprintln!("\n[{}] Resolution change failed: {}", key, e),
            }
        } else {
            // Windowed: just resize the window
            let sizes = [(1024u32, 576u32), (1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)];
            if let Some(&(w, h)) = sizes.get((key - 1) as usize) {
                if let Some(window) = &self.window {
                    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(w, h));
                    println!("\n[{}] Window size -> {}x{}", key, w, h);
                }
            }
        }
    }

    fn handle_key(&mut self, key: PhysicalKey, event_loop: &ActiveEventLoop) {
        match key {
            PhysicalKey::Code(KeyCode::Escape) | PhysicalKey::Code(KeyCode::KeyQ) => {
                println!("\nExiting...");
                event_loop.exit();
            }
            PhysicalKey::Code(KeyCode::ArrowLeft) => {
                self.current_shader_idx = self.shader_manager.prev(self.current_shader_idx);
                self.reload_requested = true;
                println!(
                    "\n<< Previous shader: {}",
                    self.shader_manager.get(self.current_shader_idx).unwrap().name
                );
            }
            PhysicalKey::Code(KeyCode::ArrowRight) => {
                self.current_shader_idx = self.shader_manager.next(self.current_shader_idx);
                self.reload_requested = true;
                println!(
                    "\n>> Next shader: {}",
                    self.shader_manager.get(self.current_shader_idx).unwrap().name
                );
            }
            PhysicalKey::Code(KeyCode::KeyF) => {
                if let Some(window) = &self.window {
                    let is_fullscreen = window.fullscreen().is_some();
                    if is_fullscreen {
                        let size = window.inner_size();
                        self.resolution_manager.restore();
                        window.set_fullscreen(None);
                        let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(size.width, size.height));
                        println!("\n[F] Windowed mode at {}x{}", size.width, size.height);
                    } else {
                        use winit::window::Fullscreen;
                        if let Some(monitor) = window.current_monitor() {
                            window.set_fullscreen(Some(Fullscreen::Borderless(Some(monitor))));
                            println!("\n[F] Fullscreen mode");
                        }
                    }
                }
            }
            PhysicalKey::Code(KeyCode::Digit1) => self.change_resolution(1),
            PhysicalKey::Code(KeyCode::Digit2) => self.change_resolution(2),
            PhysicalKey::Code(KeyCode::Digit3) => self.change_resolution(3),
            PhysicalKey::Code(KeyCode::Digit4) => self.change_resolution(4),
            PhysicalKey::Code(KeyCode::Digit5) => self.change_resolution(5),
            PhysicalKey::Code(KeyCode::KeyR) => {
                let elapsed = self.start_time.elapsed().as_secs_f32();
                self.scroll_x = 0.0;
                self.scroll_y = elapsed;  // For auto-zoom shaders: reset time offset
                self.pan_offset_x = 0.0;
                self.pan_offset_y = 0.0;
                self.base_pan_x = 0.0;
                self.base_pan_y = 0.0;
                println!("\n[R] Reset zoom and pan");
            }
            PhysicalKey::Code(KeyCode::Equal) | PhysicalKey::Code(KeyCode::NumpadAdd) => {
                self.scroll_y += 1.0;
                println!("\n[+] Zoom in: {:.1}", self.scroll_y);
            }
            PhysicalKey::Code(KeyCode::Minus) | PhysicalKey::Code(KeyCode::NumpadSubtract) => {
                self.scroll_y -= 1.0;
                println!("\n[-] Zoom out: {:.1}", self.scroll_y);
            }
            _ => {}
        }
    }
}

impl ApplicationHandler for MetalshaderApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("Metalshader - Vulkan Shader Viewer")
                .with_inner_size(winit::dpi::PhysicalSize::new(1280, 800));

            let window = match event_loop.create_window(window_attributes) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    eprintln!("Failed to create window: {}", e);
                    event_loop.exit();
                    return;
                }
            };

            // Create renderer with swapchain
            match SwapchainRenderer::new(window.clone()) {
                Ok(renderer) => {
                    println!(
                        "Metalshader on {} ({}x{})",
                        renderer.get_device_name(),
                        window.inner_size().width,
                        window.inner_size().height
                    );
                    self.renderer = Some(renderer);
                }
                Err(e) => {
                    eprintln!("Failed to create renderer: {}", e);
                    event_loop.exit();
                    return;
                }
            }

            self.window = Some(window);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                println!("\nExiting...");
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    self.handle_key(event.physical_key, event_loop);
                }
            }
            WindowEvent::RedrawRequested => {
                // Handle shader reload
                if self.reload_requested {
                    if let Some(renderer) = &mut self.renderer {
                        if let Some(shader_info) = self.shader_manager.get(self.current_shader_idx) {
                            match renderer.load_shader(
                                shader_info.vert_path.to_str().unwrap(),
                                shader_info.frag_path.to_str().unwrap()
                            ) {
                                Ok(_) => {
                                    println!("Loaded shader: {}", shader_info.name);
                                    if let Some(window) = &self.window {
                                        window.set_title(&format!("Metalshader - {}", shader_info.name));
                                    }
                                    self.reload_requested = false;
                                }
                                Err(e) => {
                                    eprintln!("Failed to load shader '{}': {}", shader_info.name, e);
                                }
                            }
                        } else {
                            eprintln!("No shaders available to load");
                            self.reload_requested = false;
                        }
                    }
                }

                // Render frame
                if let Some(renderer) = &mut self.renderer {
                    if let Some(window) = &self.window {
                        let size = window.inner_size();
                        let elapsed = self.start_time.elapsed().as_secs_f32();

                        // Update button press durations
                        let now = Instant::now();
                        let delta_time = now.duration_since(self.last_frame_time).as_secs_f32();
                        self.last_frame_time = now;

                        if self.mouse_left_pressed {
                            self.button_press_duration[0] += delta_time;
                        }
                        if self.mouse_right_pressed {
                            self.button_press_duration[1] += delta_time;
                        }
                        if self.mouse_middle_pressed {
                            self.button_press_duration[2] += delta_time;
                        }

                        // Generic mouse smoothing (shader-agnostic)
                        // Provides comfortable smoothing without viewer needing to know zoom logic
                        // Time constant: ~200ms (feels natural, reduces jitter without lag)
                        const SMOOTH_FACTOR: f32 = 0.08;  // 8% blend per frame @ 60fps
                        let smooth_speed = (SMOOTH_FACTOR * delta_time * 60.0).min(1.0);

                        self.mouse_smooth_x += (self.mouse_x - self.mouse_smooth_x) * smooth_speed.min(1.0) as f64;
                        self.mouse_smooth_y += (self.mouse_y - self.mouse_smooth_y) * smooth_speed.min(1.0) as f64;

                        // Scale mouse coordinates for Retina displays
                        let scale_x = size.width as f32 / window.inner_size().width as f32;
                        let scale_y = size.height as f32 / window.inner_size().height as f32;

                        // Use smoothed mouse position for shader
                        let scaled_mouse_x = self.mouse_smooth_x as f32 * scale_x;
                        let scaled_mouse_y = self.mouse_smooth_y as f32 * scale_y;
                        let scaled_click_x = self.mouse_click_x as f32 * scale_x;
                        let scaled_click_y = self.mouse_click_y as f32 * scale_y;

                        // ShaderToy mouse convention:
                        // xy = current position, zw = click position (negative if button up)
                        let i_mouse = if self.mouse_left_pressed {
                            [scaled_mouse_x, scaled_mouse_y, scaled_click_x, scaled_click_y]
                        } else {
                            [scaled_mouse_x, scaled_mouse_y, -scaled_click_x, -scaled_click_y]
                        };

                        // pan_offset is now in pixels, passed directly to shader
                        // Shader handles conversion to complex-plane coordinates

                        let ubo = ShaderToyUBO {
                            i_resolution: [size.width as f32, size.height as f32, 1.0],
                            i_time: elapsed,
                            i_mouse,
                            i_scroll: [self.scroll_x, self.scroll_y],
                            i_button_left: self.button_press_duration[0],
                            i_button_right: self.button_press_duration[1],
                            i_button_middle: self.button_press_duration[2],
                            i_button_4: self.button_press_duration[3],
                            i_button_5: self.button_press_duration[4],
                            i_pan: [self.pan_offset_x, self.pan_offset_y],
                        };

                        match renderer.render_frame(&ubo) {
                            Ok(_) => {
                                self.frame_count += 1;
                                if self.frame_count % 600 == 0 {
                                    let fps = self.frame_count as f32 / elapsed;
                                    println!(
                                        "{:.1}s: {} frames ({:.1} FPS) - {}",
                                        elapsed,
                                        self.frame_count,
                                        fps,
                                        self.shader_manager.get(self.current_shader_idx).unwrap().name
                                    );
                                }
                            }
                            Err(e) => {
                                eprintln!("Render error: {}", e);
                            }
                        }

                        window.request_redraw();
                    }
                }
            }
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    if let Some(renderer) = &mut self.renderer {
                        match renderer.recreate_swapchain() {
                            Ok(_) => println!("Swapchain recreated for {}x{}", new_size.width, new_size.height),
                            Err(e) => eprintln!("Failed to recreate swapchain: {}", e),
                        }
                    }
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                // Display resolution/DPI changed — swapchain must be recreated
                if let Some(renderer) = &mut self.renderer {
                    if let Err(e) = renderer.recreate_swapchain() {
                        eprintln!("Failed to recreate swapchain on scale change: {}", e);
                    }
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_x = position.x;
                self.mouse_y = position.y;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                use winit::event::MouseButton;
                let pressed = state == ElementState::Pressed;

                match button {
                    MouseButton::Left => {
                        if pressed {
                            self.mouse_left_pressed = true;
                            self.mouse_click_x = self.mouse_x;
                            self.mouse_click_y = self.mouse_y;
                            self.button_press_duration[0] = 0.0;
                        } else {
                            // Mouse released - no drag handling needed (zoom follows cursor directly)
                            self.mouse_left_pressed = false;
                            self.button_press_duration[0] = 0.0;
                        }
                    }
                    MouseButton::Right => {
                        self.mouse_right_pressed = pressed;
                        self.button_press_duration[1] = 0.0;
                    }
                    MouseButton::Middle => {
                        self.mouse_middle_pressed = pressed;
                        self.button_press_duration[2] = 0.0;
                    }
                    MouseButton::Back => {
                        self.button_press_duration[3] = 0.0;
                    }
                    MouseButton::Forward => {
                        self.button_press_duration[4] = 0.0;
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                use winit::event::MouseScrollDelta;
                match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        self.scroll_x += x;
                        self.scroll_y += y;
                    }
                    MouseScrollDelta::PixelDelta(pos) => {
                        self.scroll_x += (pos.x / 10.0) as f32;
                        self.scroll_y += (pos.y / 10.0) as f32;
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Check for Finder "Open With" file requests arriving via Apple Event
        if let Ok(mut guard) = PENDING_FILE.lock() {
            if let Some(path) = guard.take() {
                let base = MetalshaderApp::shader_name_from_path(&path);
                if let Some(idx) = self.shader_manager.find_by_name(&base) {
                    self.current_shader_idx = idx;
                    self.reload_requested = true;
                    self.start_time = Instant::now();
                    self.scroll_y = 0.0;
                }
            }
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl MetalshaderApp {
    fn shader_name_from_path(path: &str) -> String {
        let stem1 = std::path::Path::new(path)
            .file_stem().and_then(|s| s.to_str()).unwrap_or(path);
        if stem1.ends_with(".vert") || stem1.ends_with(".frag") || stem1.ends_with(".glsl") {
            std::path::Path::new(stem1).file_stem()
                .and_then(|s| s.to_str()).unwrap_or(stem1).to_string()
        } else {
            stem1.to_string()
        }
    }
}

/// If running from a bundle, set DYLD_LIBRARY_PATH and VK_ICD_FILENAMES so Vulkan loads.
fn setup_bundle_env() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(macos_dir) = exe.parent() {
            let contents = macos_dir.join("..");
            let frameworks = contents.join("Frameworks");
            let icd = contents.join("Resources/vulkan/icd.d/MoltenVK_icd.json");
            if frameworks.exists() {
                let cur = std::env::var("DYLD_LIBRARY_PATH").unwrap_or_default();
                let new_val = if cur.is_empty() {
                    frameworks.to_string_lossy().into_owned()
                } else {
                    format!("{}:{}", frameworks.to_string_lossy(), cur)
                };
                // DYLD_LIBRARY_PATH can't be changed after launch on macOS (SIP),
                // but we set it for child processes / re-exec scenario.
                unsafe { std::env::set_var("DYLD_LIBRARY_PATH", &new_val) };
            }
            if icd.exists() && std::env::var("VK_ICD_FILENAMES").is_err() {
                unsafe { std::env::set_var("VK_ICD_FILENAMES", icd.to_string_lossy().as_ref()) };
            }
        }
    }
}

pub fn run_macos(shader_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    setup_bundle_env();
    // Attempt injection before EventLoop::new() - might be too early if class not registered
    inject_open_file_handler();
    let event_loop = EventLoop::new()?;
    // Retry after EventLoop::new() in case WinitApplicationDelegate wasn't registered yet
    inject_open_file_handler();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = MetalshaderApp::new(shader_path);
    event_loop.run_app(&mut app)?;

    Ok(())
}
