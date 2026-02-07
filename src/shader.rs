// Shader discovery and management

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ShaderInfo {
    pub name: String,
    pub vert_path: PathBuf,
    pub frag_path: PathBuf,
}

pub struct ShaderManager {
    shaders: Vec<ShaderInfo>,
}

impl ShaderManager {
    pub fn new() -> Self {
        Self {
            shaders: Vec::new(),
        }
    }

    pub fn scan_shaders(&mut self, dirs: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        self.shaders.clear();

        for dir in dirs {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                        continue;
                    }

                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if ext != "frag" {
                            continue;
                        }
                    } else {
                        continue;
                    }

                    // Extract base name
                    let file_name = path.file_stem().and_then(|s| s.to_str());
                    if file_name.is_none() {
                        continue;
                    }
                    let base_name = file_name.unwrap();

                    // Build shader paths
                    let vert_path = Path::new(dir).join(format!("{}.vert.spv", base_name));
                    let frag_path = Path::new(dir).join(format!("{}.frag.spv", base_name));

                    // Check if both compiled shaders exist
                    if vert_path.exists() && frag_path.exists() {
                        self.shaders.push(ShaderInfo {
                            name: base_name.to_string(),
                            vert_path,
                            frag_path,
                        });
                    }
                }
            }
        }

        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.shaders.is_empty()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.shaders.len()
    }

    pub fn get(&self, index: usize) -> Option<&ShaderInfo> {
        self.shaders.get(index)
    }

    pub fn find_by_name(&self, name: &str) -> Option<usize> {
        self.shaders.iter().position(|s| s.name == name)
    }

    pub fn next(&self, current: usize) -> usize {
        (current + 1) % self.shaders.len()
    }

    pub fn prev(&self, current: usize) -> usize {
        if current == 0 {
            self.shaders.len() - 1
        } else {
            current - 1
        }
    }

    pub fn print_available(&self) {
        println!("Found {} compiled shader(s)", self.shaders.len());
        for (i, shader) in self.shaders.iter().enumerate() {
            println!("  [{}] {}", i, shader.name);
        }
    }
}
