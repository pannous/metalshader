// Automatic shader compilation support
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;

pub struct ShaderCompiler {
    #[allow(dead_code)]
    shader_dir: PathBuf,
}

impl ShaderCompiler {
    pub fn new() -> Self {
        Self {
            shader_dir: PathBuf::from("."),
        }
    }

    /// Try to compile a shader from source (.frag, .glsl) to SPIR-V (.spv)
    /// Returns the path to the compiled SPIR-V files (base name)
    pub fn compile_if_needed(&self, input_path: &str) -> Result<String, Box<dyn std::error::Error>> {
        let input = Path::new(input_path);

        // Check if file exists
        if !input.exists() {
            return Err(format!("Shader file not found: {}", input_path).into());
        }

        // Determine the base name and directory
        let base_name = input
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid shader path")?
            .to_string();

        let shader_dir = input
            .parent()
            .unwrap_or_else(|| Path::new("."));

        // Check if we have SPIR-V files already
        let vert_spv = shader_dir.join(format!("{}.vert.spv", base_name));
        let frag_spv = shader_dir.join(format!("{}.frag.spv", base_name));

        if vert_spv.exists() && frag_spv.exists() {
            println!("✓ Using existing SPIR-V: {}", frag_spv.display());
            return Ok(base_name);
        }

        // Need to compile - check if input is a GLSL file
        if let Some(ext) = input.extension().and_then(|s| s.to_str()) {
            match ext {
                "frag" | "glsl" | "fsh" => {
                    // Fragment shader source
                    println!("Compiling shader: {} -> {}", input_path, frag_spv.display());
                    self.compile_glsl_to_spirv(input, &base_name, shader_dir)?;
                    return Ok(base_name);
                }
                "spv" => {
                    // Already SPIR-V
                    return Ok(base_name);
                }
                _ => {
                    return Err(format!("Unknown shader extension: {}", ext).into());
                }
            }
        }

        Err("Could not determine shader type".into())
    }

    fn compile_glsl_to_spirv(
        &self,
        input: &Path,
        base_name: &str,
        output_dir: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Step 1: Convert to Vulkan GLSL if needed
        let vulkan_glsl = if self.is_vulkan_ready(input)? {
            input.to_path_buf()
        } else {
            let temp_glsl = output_dir.join(format!("{}.glsl", base_name));
            self.convert_to_vulkan_glsl(input, &temp_glsl)?;
            temp_glsl
        };

        // Step 2: Generate vertex shader if not present
        let vert_glsl = output_dir.join(format!("{}.vert", base_name));
        if !vert_glsl.exists() {
            self.generate_fullscreen_vertex_shader(&vert_glsl)?;
        }

        // Step 3: Compile to SPIR-V
        let frag_spv = output_dir.join(format!("{}.frag.spv", base_name));
        let vert_spv = output_dir.join(format!("{}.vert.spv", base_name));

        self.compile_glslang(&vulkan_glsl, &frag_spv, "frag")?;
        self.compile_glslang(&vert_glsl, &vert_spv, "vert")?;

        println!("✓ Compiled: {}", frag_spv.display());
        println!("✓ Compiled: {}", vert_spv.display());

        Ok(())
    }

    fn is_vulkan_ready(&self, path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        Ok(content.contains("#version 450"))
    }

    fn convert_to_vulkan_glsl(
        &self,
        input: &Path,
        output: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let content = fs::read_to_string(input)?;

        // Basic conversion: wrap in Vulkan boilerplate
        let vulkan_shader = format!(
            r#"#version 450

layout(location = 0) in vec2 fragCoord;
layout(location = 0) out vec4 fragColor;

layout(binding = 0, set = 0) uniform UniformBufferObject {{
    vec3 iResolution;
    float iTime;
    vec4 iMouse;
}} ubo;

layout(binding = 1, set = 0) uniform sampler2D iChannel0;

{}
"#,
            content
        );

        fs::write(output, vulkan_shader)?;
        Ok(())
    }

    fn generate_fullscreen_vertex_shader(
        &self,
        output: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let vert_shader = r#"#version 450

layout(location = 0) out vec2 fragCoord;

layout(binding = 0, set = 0) uniform UniformBufferObject {
    vec3 iResolution;
    float iTime;
    vec4 iMouse;
} ubo;

void main() {
    vec2 positions[6] = vec2[](
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0)
    );
    gl_Position = vec4(positions[gl_VertexIndex], 0.0, 1.0);
    fragCoord = (positions[gl_VertexIndex] * 0.5 + 0.5) * ubo.iResolution.xy;
}
"#;

        fs::write(output, vert_shader)?;
        Ok(())
    }

    fn compile_glslang(
        &self,
        input: &Path,
        output: &Path,
        stage: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if glslangValidator exists
        let check = Command::new("which")
            .arg("glslangValidator")
            .output()?;

        if !check.status.success() {
            return Err("glslangValidator not found. Install with: brew install glslang".into());
        }

        let output_result = Command::new("glslangValidator")
            .arg("-V")
            .arg(input)
            .arg("-o")
            .arg(output)
            .output()?;

        if !output_result.status.success() {
            let stderr = String::from_utf8_lossy(&output_result.stderr);
            eprintln!("Compilation error:\n{}", stderr);
            return Err(format!("Failed to compile {} shader", stage).into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shader_compiler() {
        let compiler = ShaderCompiler::new();
        // Test would go here
    }
}
