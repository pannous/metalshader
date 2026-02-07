// Vulkan rendering engine

use ash::vk;
use std::ffi::CStr;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub struct VulkanRenderer {
    entry: ash::Entry,
    instance: ash::Instance,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    queue: vk::Queue,

    render_target_image: vk::Image,
    render_target_memory: vk::DeviceMemory,
    render_target_view: vk::ImageView,
    render_target_ptr: *mut u8,
    render_target_size: usize,

    texture_image: vk::Image,
    texture_memory: vk::DeviceMemory,
    texture_view: vk::ImageView,
    sampler: vk::Sampler,

    render_pass: vk::RenderPass,
    framebuffer: vk::Framebuffer,

    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,

    uniform_buffer: vk::Buffer,
    uniform_memory: vk::DeviceMemory,
    uniform_ptr: *mut u8,

    pipeline: Option<vk::Pipeline>,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,

    width: u32,
    height: u32,
    row_pitch: usize,
}

impl VulkanRenderer {
    pub fn new(width: u32, height: u32) -> Result<Self, Box<dyn std::error::Error>> {
        unsafe {
            let entry = ash::Entry::load()?;

            // Create instance
            let app_info = vk::ApplicationInfo::default()
                .api_version(vk::make_api_version(0, 1, 2, 0));

            let create_info = vk::InstanceCreateInfo::default()
                .application_info(&app_info);

            let instance = entry.create_instance(&create_info, None)?;

            // Get physical device
            let physical_devices = instance.enumerate_physical_devices()?;
            let physical_device = *physical_devices.first()
                .ok_or("No Vulkan physical device found")?;

            let mem_properties = instance.get_physical_device_memory_properties(physical_device);

            // Create device
            let queue_info = vk::DeviceQueueCreateInfo::default()
                .queue_family_index(0)
                .queue_priorities(&[1.0]);

            let device_create_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(std::slice::from_ref(&queue_info));

            let device = instance.create_device(physical_device, &device_create_info, None)?;
            let queue = device.get_device_queue(0, 0);

            // Create render target image (LINEAR + HOST_VISIBLE)
            let rt_image_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::B8G8R8A8_UNORM)
                .extent(vk::Extent3D { width, height, depth: 1 })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::LINEAR)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .initial_layout(vk::ImageLayout::UNDEFINED);

            let render_target_image = device.create_image(&rt_image_info, None)?;
            let rt_mem_req = device.get_image_memory_requirements(render_target_image);

            let rt_mem_type = find_memory_type(
                &mem_properties,
                rt_mem_req.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;

            let rt_alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(rt_mem_req.size)
                .memory_type_index(rt_mem_type);

            let render_target_memory = device.allocate_memory(&rt_alloc_info, None)?;
            device.bind_image_memory(render_target_image, render_target_memory, 0)?;

            let render_target_ptr = device.map_memory(
                render_target_memory,
                0,
                vk::WHOLE_SIZE,
                vk::MemoryMapFlags::empty(),
            )? as *mut u8;

            let rt_view_info = vk::ImageViewCreateInfo::default()
                .image(render_target_image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::B8G8R8A8_UNORM)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let render_target_view = device.create_image_view(&rt_view_info, None)?;

            // Get layout for row pitch
            let subresource = vk::ImageSubresource {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                array_layer: 0,
            };
            let layout = device.get_image_subresource_layout(render_target_image, subresource);
            let row_pitch = layout.row_pitch as usize;

            // Create texture
            let (texture_image, texture_memory, texture_view) =
                Self::create_texture(&device, &mem_properties)?;

            // Create sampler
            let sampler_info = vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .address_mode_w(vk::SamplerAddressMode::REPEAT);

            let sampler = device.create_sampler(&sampler_info, None)?;

            // Create render pass
            let attachment = vk::AttachmentDescription::default()
                .format(vk::Format::B8G8R8A8_UNORM)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::GENERAL);

            let color_ref = vk::AttachmentReference::default()
                .attachment(0)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

            let subpass = vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(std::slice::from_ref(&color_ref));

            let render_pass_info = vk::RenderPassCreateInfo::default()
                .attachments(std::slice::from_ref(&attachment))
                .subpasses(std::slice::from_ref(&subpass));

            let render_pass = device.create_render_pass(&render_pass_info, None)?;

            // Create framebuffer
            let fb_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(std::slice::from_ref(&render_target_view))
                .width(width)
                .height(height)
                .layers(1);

            let framebuffer = device.create_framebuffer(&fb_info, None)?;

            // Create uniform buffer
            let ubo_size = 64;
            let ubo_info = vk::BufferCreateInfo::default()
                .size(ubo_size)
                .usage(vk::BufferUsageFlags::UNIFORM_BUFFER);

            let uniform_buffer = device.create_buffer(&ubo_info, None)?;
            let ubo_req = device.get_buffer_memory_requirements(uniform_buffer);

            let ubo_mem_type = find_memory_type(
                &mem_properties,
                ubo_req.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;

            let ubo_alloc = vk::MemoryAllocateInfo::default()
                .allocation_size(ubo_req.size)
                .memory_type_index(ubo_mem_type);

            let uniform_memory = device.allocate_memory(&ubo_alloc, None)?;
            device.bind_buffer_memory(uniform_buffer, uniform_memory, 0)?;

            let uniform_ptr = device.map_memory(
                uniform_memory,
                0,
                ubo_size,
                vk::MemoryMapFlags::empty(),
            )? as *mut u8;

            // Create descriptors
            let bindings = [
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            ];

            let desc_layout_info = vk::DescriptorSetLayoutCreateInfo::default()
                .bindings(&bindings);

            let descriptor_set_layout = device.create_descriptor_set_layout(&desc_layout_info, None)?;

            let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
                .set_layouts(std::slice::from_ref(&descriptor_set_layout));

            let pipeline_layout = device.create_pipeline_layout(&pipeline_layout_info, None)?;

            // Create descriptor pool
            let pool_sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: 1,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: 1,
                },
            ];

            let pool_info = vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&pool_sizes);

            let descriptor_pool = device.create_descriptor_pool(&pool_info, None)?;

            let alloc_info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool)
                .set_layouts(std::slice::from_ref(&descriptor_set_layout));

            let descriptor_sets = device.allocate_descriptor_sets(&alloc_info)?;
            let descriptor_set = descriptor_sets[0];

            // Update descriptors
            let buffer_info = vk::DescriptorBufferInfo::default()
                .buffer(uniform_buffer)
                .offset(0)
                .range(64);

            let image_info = vk::DescriptorImageInfo::default()
                .sampler(sampler)
                .image_view(texture_view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(std::slice::from_ref(&buffer_info)),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(std::slice::from_ref(&image_info)),
            ];

            device.update_descriptor_sets(&writes, &[]);

            // Create command pool
            let pool_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(0);

            let command_pool = device.create_command_pool(&pool_info, None)?;

            let alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);

            let command_buffers = device.allocate_command_buffers(&alloc_info)?;
            let command_buffer = command_buffers[0];

            // Transition texture to shader read
            Self::transition_texture_layout(
                &device,
                command_buffer,
                queue,
                texture_image,
            )?;

            // Create fence
            let fence_info = vk::FenceCreateInfo::default();
            let fence = device.create_fence(&fence_info, None)?;

            Ok(Self {
                entry,
                instance,
                device,
                physical_device,
                queue,
                render_target_image,
                render_target_memory,
                render_target_view,
                render_target_ptr,
                render_target_size: (height as usize * row_pitch),
                texture_image,
                texture_memory,
                texture_view,
                sampler,
                render_pass,
                framebuffer,
                descriptor_pool,
                descriptor_set_layout,
                descriptor_set,
                pipeline_layout,
                uniform_buffer,
                uniform_memory,
                uniform_ptr,
                pipeline: None,
                command_pool,
                command_buffer,
                fence,
                width,
                height,
                row_pitch,
            })
        }
    }

    pub fn get_device_name(&self) -> String {
        unsafe {
            let props = self.instance.get_physical_device_properties(self.physical_device);
            CStr::from_ptr(props.device_name.as_ptr())
                .to_string_lossy()
                .to_string()
        }
    }

    pub fn load_shader(&mut self, vert_path: &Path, frag_path: &Path)
        -> Result<(), Box<dyn std::error::Error>>
    {
        unsafe {
            // Destroy old pipeline if exists
            if let Some(pipeline) = self.pipeline.take() {
                self.device.destroy_pipeline(pipeline, None);
            }

            // Load shader code
            let vert_code = load_shader_code(vert_path)?;
            let frag_code = load_shader_code(frag_path)?;

            // Create shader modules
            let vert_info = vk::ShaderModuleCreateInfo::default()
                .code(&vert_code);
            let vert_module = self.device.create_shader_module(&vert_info, None)?;

            let frag_info = vk::ShaderModuleCreateInfo::default()
                .code(&frag_code);
            let frag_module = self.device.create_shader_module(&frag_info, None)?;

            let entry_name = CStr::from_bytes_with_nul(b"main\0")?;

            let stages = [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vert_module)
                    .name(entry_name),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(frag_module)
                    .name(entry_name),
            ];

            let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();

            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.width as f32,
                height: self.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };

            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: self.width,
                    height: self.height,
                },
            };

            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewports(std::slice::from_ref(&viewport))
                .scissors(std::slice::from_ref(&scissor));

            let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::NONE)
                .line_width(1.0);

            let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);

            let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA);

            let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
                .attachments(std::slice::from_ref(&color_blend_attachment));

            let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterizer)
                .multisample_state(&multisampling)
                .color_blend_state(&color_blending)
                .layout(self.pipeline_layout)
                .render_pass(self.render_pass)
                .subpass(0);

            let pipelines = self.device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                std::slice::from_ref(&pipeline_info),
                None,
            ).map_err(|e| e.1)?;

            self.pipeline = Some(pipelines[0]);

            // Clean up shader modules
            self.device.destroy_shader_module(vert_module, None);
            self.device.destroy_shader_module(frag_module, None);

            Ok(())
        }
    }

    pub fn render_frame(&mut self, ubo: &crate::ShaderToyUBO)
        -> Result<(), Box<dyn std::error::Error>>
    {
        unsafe {
            let pipeline = self.pipeline.ok_or("No shader loaded")?;

            // Update UBO
            std::ptr::copy_nonoverlapping(
                ubo as *const _ as *const u8,
                self.uniform_ptr,
                std::mem::size_of::<crate::ShaderToyUBO>(),
            );

            // Record commands
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

            self.device.begin_command_buffer(self.command_buffer, &begin_info)?;

            let clear_value = vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            };

            let render_pass_info = vk::RenderPassBeginInfo::default()
                .render_pass(self.render_pass)
                .framebuffer(self.framebuffer)
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: vk::Extent2D {
                        width: self.width,
                        height: self.height,
                    },
                })
                .clear_values(std::slice::from_ref(&clear_value));

            self.device.cmd_begin_render_pass(
                self.command_buffer,
                &render_pass_info,
                vk::SubpassContents::INLINE,
            );

            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );

            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_set],
                &[],
            );

            self.device.cmd_draw(self.command_buffer, 6, 1, 0, 0);
            self.device.cmd_end_render_pass(self.command_buffer);
            self.device.end_command_buffer(self.command_buffer)?;

            // Submit and wait
            let submit_info = vk::SubmitInfo::default()
                .command_buffers(std::slice::from_ref(&self.command_buffer));

            self.device.queue_submit(self.queue, &[submit_info], self.fence)?;
            self.device.wait_for_fences(&[self.fence], true, u64::MAX)?;
            self.device.reset_fences(&[self.fence])?;

            Ok(())
        }
    }

    pub fn get_frame_buffer(&self) -> &[u8] {
        unsafe {
            let buffer = std::slice::from_raw_parts(self.render_target_ptr, self.render_target_size);

            // Debug: check first few pixels
            if buffer.len() >= 16 {
                let first_pixels: Vec<u8> = buffer[0..16].to_vec();
                eprintln!("First 16 bytes of framebuffer: {:02x?}", first_pixels);
                eprintln!("Row pitch: {}, Width: {}, Expected: {}",
                    self.row_pitch, self.width, self.width * 4);
            }

            buffer
        }
    }

    pub fn get_row_pitch(&self) -> usize {
        self.row_pitch
    }

    // DEBUG: Fill framebuffer with test pattern
    pub fn fill_test_pattern(&mut self) {
        unsafe {
            let buffer = std::slice::from_raw_parts_mut(self.render_target_ptr, self.render_target_size);
            for y in 0..self.height as usize {
                for x in 0..self.width as usize {
                    let offset = y * self.row_pitch + x * 4;
                    if offset + 3 < buffer.len() {
                        // Checkerboard pattern
                        let checker = ((x / 64) + (y / 64)) % 2;
                        buffer[offset + 0] = if checker == 1 { 255 } else { 0 }; // B
                        buffer[offset + 1] = if checker == 1 { 0 } else { 255 }; // G
                        buffer[offset + 2] = 0; // R
                        buffer[offset + 3] = 255; // A
                    }
                }
            }
            eprintln!("Filled test pattern: {}x{} with row_pitch {}", self.width, self.height, self.row_pitch);
        }
    }

    fn create_texture(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
    ) -> Result<(vk::Image, vk::DeviceMemory, vk::ImageView), Box<dyn std::error::Error>> {
        unsafe {
            let tex_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .extent(vk::Extent3D { width: 256, height: 256, depth: 1 })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::LINEAR)
                .usage(vk::ImageUsageFlags::SAMPLED)
                .initial_layout(vk::ImageLayout::PREINITIALIZED);

            let texture_image = device.create_image(&tex_info, None)?;
            let tex_req = device.get_image_memory_requirements(texture_image);

            let tex_mem_type = find_memory_type(
                mem_props,
                tex_req.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;

            let tex_alloc = vk::MemoryAllocateInfo::default()
                .allocation_size(tex_req.size)
                .memory_type_index(tex_mem_type);

            let texture_memory = device.allocate_memory(&tex_alloc, None)?;
            device.bind_image_memory(texture_image, texture_memory, 0)?;

            // Upload texture data
            let ptr = device.map_memory(
                texture_memory,
                0,
                vk::WHOLE_SIZE,
                vk::MemoryMapFlags::empty(),
            )? as *mut u8;

            let subresource = vk::ImageSubresource {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                array_layer: 0,
            };
            let layout = device.get_image_subresource_layout(texture_image, subresource);

            // Generate checkerboard
            let mut tex_data = vec![0u8; 256 * 256 * 4];
            for y in 0..256 {
                for x in 0..256 {
                    let idx = (y * 256 + x) * 4;
                    let checker = ((x / 32) + (y / 32)) % 2;
                    tex_data[idx] = if checker != 0 { 200 } else { 50 };
                    tex_data[idx + 1] = if checker != 0 { 180 } else { 60 };
                    tex_data[idx + 2] = if checker != 0 { 160 } else { 80 };
                    tex_data[idx + 3] = 255;
                }
            }

            for y in 0..256 {
                let dst = ptr.add(y * layout.row_pitch as usize);
                let src = tex_data.as_ptr().add(y * 256 * 4);
                std::ptr::copy_nonoverlapping(src, dst, 256 * 4);
            }

            device.unmap_memory(texture_memory);

            let view_info = vk::ImageViewCreateInfo::default()
                .image(texture_image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let texture_view = device.create_image_view(&view_info, None)?;

            Ok((texture_image, texture_memory, texture_view))
        }
    }

    fn transition_texture_layout(
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        queue: vk::Queue,
        image: vk::Image,
    ) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            let begin_info = vk::CommandBufferBeginInfo::default();
            device.begin_command_buffer(cmd, &begin_info)?;

            let barrier = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::HOST_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::PREINITIALIZED)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );

            device.end_command_buffer(cmd)?;

            let fence_info = vk::FenceCreateInfo::default();
            let fence = device.create_fence(&fence_info, None)?;

            let submit_info = vk::SubmitInfo::default()
                .command_buffers(std::slice::from_ref(&cmd));

            device.queue_submit(queue, &[submit_info], fence)?;
            device.wait_for_fences(&[fence], true, u64::MAX)?;
            device.destroy_fence(fence, None);

            Ok(())
        }
    }
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();

            if let Some(pipeline) = self.pipeline {
                self.device.destroy_pipeline(pipeline, None);
            }

            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_buffer(self.uniform_buffer, None);
            self.device.free_memory(self.uniform_memory, None);
            self.device.destroy_framebuffer(self.framebuffer, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.device.destroy_sampler(self.sampler, None);
            self.device.destroy_image_view(self.texture_view, None);
            self.device.destroy_image(self.texture_image, None);
            self.device.free_memory(self.texture_memory, None);
            self.device.destroy_image_view(self.render_target_view, None);
            self.device.destroy_image(self.render_target_image, None);
            self.device.free_memory(self.render_target_memory, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

fn find_memory_type(
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    type_bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> Result<u32, Box<dyn std::error::Error>> {
    for i in 0..mem_props.memory_type_count {
        if (type_bits & (1 << i)) != 0
            && mem_props.memory_types[i as usize].property_flags.contains(flags)
        {
            return Ok(i);
        }
    }
    Err("No suitable memory type found".into())
}

fn load_shader_code(path: &Path) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    // Convert bytes to u32
    let code: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    Ok(code)
}
