// macOS-specific main with windowed swapchain support
#![cfg(target_os = "macos")]

use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::renderer_swapchain::SwapchainRenderer;
use crate::shader::ShaderManager;
use crate::shader_compiler::ShaderCompiler;

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
    current_shader_idx: usize,
    start_time: Instant,
    frame_count: u32,
    reload_requested: bool,
    // Mouse and scroll state
    mouse_x: f64,
    mouse_y: f64,
    mouse_click_x: f64,
    mouse_click_y: f64,
    mouse_left_pressed: bool,
    mouse_right_pressed: bool,
    mouse_middle_pressed: bool,
    button_press_duration: [f32; 5],  // Duration in seconds for each button
    scroll_x: f32,
    scroll_y: f32,
    pan_offset_x: f32,
    pan_offset_y: f32,
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

        // Scan for shaders
        if let Err(e) = shader_manager.scan_shaders(&[".", "./shaders", "/root/metalshade/shaders"]) {
            eprintln!("Warning: Failed to scan shaders: {}", e);
        }

        if shader_manager.is_empty() {
            eprintln!("No compiled shaders found.");
            eprintln!("Searched: . ./shaders /root/metalshade/shaders");
            eprintln!("Compile shaders with: glslangValidator -V <shader>.vert -o <shader>.vert.spv");
        } else {
            shader_manager.print_available();
        }

        // Extract base name from shader path for shader manager lookup
        let base_shader_path = std::path::Path::new(&resolved_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(shader_path);

        let current_shader_idx = shader_manager
            .find_by_name(base_shader_path)
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
            current_shader_idx,
            start_time: Instant::now(),
            frame_count: 0,
            reload_requested: true,
            mouse_x: 0.0,
            mouse_y: 0.0,
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
            last_frame_time: Instant::now(),
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
                        window.set_fullscreen(None);
                        println!("\n[F] Windowed mode");
                    } else {
                        use winit::window::Fullscreen;
                        if let Some(monitor) = window.current_monitor() {
                            window.set_fullscreen(Some(Fullscreen::Borderless(Some(monitor))));
                            println!("\n[F] Fullscreen mode");
                        }
                    }
                }
            }
            PhysicalKey::Code(KeyCode::Digit1) => {
                if let Some(window) = &self.window {
                    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(800, 600));
                    println!("\n[1] Resolution: 800x600");
                }
            }
            PhysicalKey::Code(KeyCode::Digit2) => {
                if let Some(window) = &self.window {
                    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(1280, 800));
                    println!("\n[2] Resolution: 1280x800");
                }
            }
            PhysicalKey::Code(KeyCode::Digit3) => {
                if let Some(window) = &self.window {
                    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(1920, 1080));
                    println!("\n[3] Resolution: 1920x1080 (Full HD)");
                }
            }
            PhysicalKey::Code(KeyCode::Digit4) => {
                if let Some(window) = &self.window {
                    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(3840, 2160));
                    println!("\n[4] Resolution: 3840x2160 (4K)");
                }
            }
            PhysicalKey::Code(KeyCode::KeyR) => {
                self.scroll_x = 0.0;
                self.scroll_y = 0.0;
                self.pan_offset_x = 0.0;
                self.pan_offset_y = 0.0;
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

            let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

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

                        // Scale mouse coordinates for Retina displays
                        let scale_x = size.width as f32 / window.inner_size().width as f32;
                        let scale_y = size.height as f32 / window.inner_size().height as f32;

                        let scaled_mouse_x = self.mouse_x as f32 * scale_x;
                        let scaled_mouse_y = self.mouse_y as f32 * scale_y;
                        let scaled_click_x = self.mouse_click_x as f32 * scale_x;
                        let scaled_click_y = self.mouse_click_y as f32 * scale_y;

                        // ShaderToy mouse convention:
                        // xy = current position, zw = click position (negative if button up)
                        let i_mouse = if self.mouse_left_pressed {
                            [scaled_mouse_x, scaled_mouse_y, scaled_click_x, scaled_click_y]
                        } else {
                            [scaled_mouse_x, scaled_mouse_y, -scaled_click_x, -scaled_click_y]
                        };

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
                                if self.frame_count % 60 == 0 {
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
                if let Some(renderer) = &mut self.renderer {
                    match renderer.recreate_swapchain() {
                        Ok(_) => {
                            println!("Swapchain recreated for {}x{}", new_size.width, new_size.height);
                            // Trigger shader reload to recreate pipeline with new viewport
                            self.reload_requested = true;
                        }
                        Err(e) => {
                            eprintln!("Failed to recreate swapchain: {}", e);
                        }
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
                            if self.mouse_left_pressed {
                                // Accumulate drag offset into pan offset
                                let drag_delta_x = self.mouse_x - self.mouse_click_x;
                                let drag_delta_y = self.mouse_y - self.mouse_click_y;
                                self.pan_offset_x += drag_delta_x as f32;
                                self.pan_offset_y += drag_delta_y as f32;
                            }
                            self.mouse_left_pressed = false;
                        }
                    }
                    MouseButton::Right => {
                        self.mouse_right_pressed = pressed;
                        if pressed {
                            self.button_press_duration[1] = 0.0;
                        }
                    }
                    MouseButton::Middle => {
                        self.mouse_middle_pressed = pressed;
                        if pressed {
                            self.button_press_duration[2] = 0.0;
                        }
                    }
                    MouseButton::Back => {
                        if pressed {
                            self.button_press_duration[3] = 0.0;
                        }
                    }
                    MouseButton::Forward => {
                        if pressed {
                            self.button_press_duration[4] = 0.0;
                        }
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
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

pub fn run_macos(shader_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = MetalshaderApp::new(shader_path);

    event_loop.run_app(&mut app)?;

    Ok(())
}
