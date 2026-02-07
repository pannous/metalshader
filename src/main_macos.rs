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
}

impl MetalshaderApp {
    fn new(shader_path: &str) -> Self {
        let mut shader_manager = ShaderManager::new();
        let shader_compiler = ShaderCompiler::new();

        // First, try to compile the requested shader if it's a source file
        if shader_path.ends_with(".frag") || shader_path.ends_with(".glsl") {
            match shader_compiler.compile_if_needed(shader_path) {
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
        let base_shader_path = std::path::Path::new(shader_path)
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

                        let ubo = ShaderToyUBO {
                            i_resolution: [size.width as f32, size.height as f32, 1.0],
                            i_time: elapsed,
                            i_mouse: [0.0, 0.0, 0.0, 0.0],
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
            WindowEvent::Resized(_new_size) => {
                // Swapchain will be recreated automatically on next frame
                if let Some(window) = &self.window {
                    window.request_redraw();
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
