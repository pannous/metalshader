// Metalshader - Interactive shader viewer in Rust
// Controls:
//   Arrow Left/Right: Switch between shaders
//   1-9: Change resolution mode
//   ESC/Q: Quit
//   F: Toggle fullscreen

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;

mod renderer;
mod shader;
mod platform;

// Platform-conditional imports
#[cfg(target_os = "linux")]
use platform::linux::{LinuxDisplay as Display, LinuxInput as Input};

#[cfg(target_os = "redox")]
use platform::redox::{RedoxDisplay as Display, RedoxInput as Input};

#[cfg(target_os = "macos")]
use platform::macos::{MacOSDisplay as Display, MacOSInput as Input};

use platform::{DisplayBackend, InputBackend, KeyEvent};
use renderer::VulkanRenderer;
use shader::ShaderManager;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct ShaderToyUBO {
    i_resolution: [f32; 3],
    i_time: f32,
    i_mouse: [f32; 4],
}

#[cfg(any(target_os = "linux", target_os = "redox", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let shader_name = if args.len() < 2 {
        "example"
    } else {
        args[1].as_str()
    };

    // Extract base name from path
    let shader_name = Path::new(shader_name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("example");

    // Initialize shader manager and scan for shaders
    let mut shader_manager = ShaderManager::new();
    shader_manager.scan_shaders(&[".", "./shaders", "/root/metalshade/shaders"])?;

    if shader_manager.is_empty() {
        eprintln!("No compiled shaders found.");
        eprintln!("Searched: . ./shaders /root/metalshade/shaders");
        eprintln!("Compile shaders with: glslangValidator -V <shader>.vert -o <shader>.vert.spv");
        return Err("No shaders found".into());
    }

    shader_manager.print_available();

    // Find requested shader
    let current_shader_idx = shader_manager
        .find_by_name(shader_name)
        .ok_or_else(|| {
            eprintln!("Shader '{}' not found. Available shaders:", shader_name);
            shader_manager.print_available();
            "Shader not found"
        })?;

    println!("Starting with shader: {}", shader_name);

    // Initialize display
    let mut display = Display::new()?;
    let (mut width, mut height) = display.get_resolution();
    println!("Display resolution: {}x{}", width, height);

    // Initialize keyboard input
    let mut keyboard = Input::new()?;

    // Initialize Vulkan renderer
    let mut renderer = VulkanRenderer::new(width, height)?;
    println!(
        "Metalshader on {} ({}x{})",
        renderer.get_device_name(),
        width,
        height
    );

    // Main loop state
    let mut current_shader_idx = current_shader_idx;
    let mut reload_requested = true;
    let start_time = Instant::now();
    let mut frame_count = 0u32;

    loop {
        // Handle shader reload
        if reload_requested {
            let shader_info = shader_manager.get(current_shader_idx).unwrap();
            match renderer.load_shader(&shader_info.vert_path, &shader_info.frag_path) {
                Ok(_) => {
                    println!("Loaded shader: {}", shader_info.name);
                    reload_requested = false;
                }
                Err(e) => {
                    eprintln!("Failed to load shader '{}': {}", shader_info.name, e);
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
            }
        }

        // Calculate time
        let elapsed = start_time.elapsed().as_secs_f32();

        // Check keyboard input
        if let Some(event) = keyboard.poll_event() {
            match event {
                KeyEvent::Left => {
                    current_shader_idx = shader_manager.prev(current_shader_idx);
                    reload_requested = true;
                    println!(
                        "\n<< Previous shader: {}",
                        shader_manager.get(current_shader_idx).unwrap().name
                    );
                }
                KeyEvent::Right => {
                    current_shader_idx = shader_manager.next(current_shader_idx);
                    reload_requested = true;
                    println!(
                        "\n>> Next shader: {}",
                        shader_manager.get(current_shader_idx).unwrap().name
                    );
                }
                KeyEvent::Resolution(mode_num) => {
                    println!("\n[{}] Changing resolution...", mode_num);
                    match display.set_mode(mode_num) {
                        Ok((new_width, new_height)) => {
                            // Recreate renderer at new resolution
                            renderer = VulkanRenderer::new(new_width, new_height)?;
                            width = new_width;
                            height = new_height;
                            reload_requested = true;
                            println!("    Resolution changed to {}x{}", new_width, new_height);
                            // Skip rendering this frame - reload shader first
                            continue;
                        }
                        Err(e) => {
                            eprintln!("    Failed to change resolution: {}", e);
                        }
                    }
                }
                KeyEvent::Fullscreen => {
                    println!("\n[F] Toggling host fullscreen...");
                    if let Err(e) = send_fullscreen_command() {
                        eprintln!("    (Can't send fullscreen command: {})", e);
                        eprintln!("    Press Ctrl+Alt+F on Mac host");
                    }
                }
                KeyEvent::Quit => {
                    println!("\nExiting...");
                    break;
                }
            }
        }

        // Update UBO
        let ubo = ShaderToyUBO {
            i_resolution: [width as f32, height as f32, 1.0],
            i_time: elapsed,
            i_mouse: [0.0, 0.0, 0.0, 0.0],
        };

        // DEBUG: Test pattern first to verify display works
        static mut TEST_DONE: bool = false;
        unsafe {
            if !TEST_DONE {
                renderer.fill_test_pattern();
                TEST_DONE = true;
            }
        }

        // Render frame
        renderer.render_frame(&ubo)?;

        // Copy to display (with correct row pitch)
        display.present(renderer.get_frame_buffer(), renderer.get_row_pitch())?;

        // Print FPS
        frame_count += 1;
        if frame_count % 60 == 0 {
            let fps = frame_count as f32 / elapsed;
            println!(
                "{:.1}s: {} frames ({:.1} FPS) - {}",
                elapsed,
                frame_count,
                fps,
                shader_manager.get(current_shader_idx).unwrap().name
            );
        }
    }

    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "redox", target_os = "macos")))]
fn main() {
    eprintln!("This platform is not supported.");
    eprintln!("Supported platforms: Linux, Redox, macOS");
}

fn send_fullscreen_command() -> Result<(), Box<dyn std::error::Error>> {
    // Find QEMU display control port
    for i in 0..10 {
        let name_path = format!("/sys/class/virtio-ports/vport{}p1/name", i);
        if let Ok(mut file) = File::open(&name_path) {
            let mut name = String::new();
            file.read_to_string(&mut name)?;
            if name.contains("org.qemu.display") {
                let port_path = format!("/dev/vport{}p1", i);
                let mut port = File::create(&port_path)?;
                port.write_all(b"FULLSCREEN\n")?;
                port.flush()?;
                return Ok(());
            }
        }
    }
    Err("Display port not found".into())
}
