// Swapchain-based Vulkan renderer for windowed mode (macOS)
#![cfg(target_os = "macos")]

use ash::vk;
use std::ffi::CStr;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use winit::window::Window;

pub struct SwapchainRenderer {
    #[allow(dead_code)]
    entry: ash::Entry,
    instance: ash::Instance,
    surface: vk::SurfaceKHR,
    surface_loader: ash::khr::surface::Instance,
    #[allow(dead_code)]
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    queue: vk::Queue,
    #[allow(dead_code)]
    queue_family_index: u32,

    swapchain: vk::SwapchainKHR,
    swapchain_loader: ash::khr::swapchain::Device,
    #[allow(dead_code)]
    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<vk::ImageView>,
    swapchain_extent: vk::Extent2D,
    #[allow(dead_code)]
    swapchain_format: vk::Format,

    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,

    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: Option<vk::Pipeline>,

    uniform_buffer: vk::Buffer,
    uniform_memory: vk::DeviceMemory,
    uniform_ptr: *mut u8,

    texture_image: vk::Image,
    texture_memory: vk::DeviceMemory,
    texture_view: vk::ImageView,
    sampler: vk::Sampler,

    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,

    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,

    image_available_semaphores: Vec<vk::Semaphore>,
    render_finished_semaphores: Vec<vk::Semaphore>,
    in_flight_fences: Vec<vk::Fence>,
    current_frame: usize,

    #[allow(dead_code)]
    window: Arc<Window>,
    device_name: String,
}

const MAX_FRAMES_IN_FLIGHT: usize = 2;

impl SwapchainRenderer {
    pub fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        unsafe {
            let entry = ash::Entry::load()?;

            // Create instance with surface extensions
            let app_info = vk::ApplicationInfo::default()
                .api_version(vk::make_api_version(0, 1, 2, 0));

            let extension_names = vec![
                ash::khr::surface::NAME.as_ptr(),
                ash::ext::metal_surface::NAME.as_ptr(),
                b"VK_KHR_portability_enumeration\0".as_ptr() as *const i8,
                b"VK_KHR_get_physical_device_properties2\0".as_ptr() as *const i8,
            ];

            let create_info = vk::InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_extension_names(&extension_names)
                .flags(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR);

            let instance = entry.create_instance(&create_info, None)?;

            // Create surface
            use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
            let surface = ash_window::create_surface(
                &entry,
                &instance,
                window.display_handle()?.as_raw(),
                window.window_handle()?.as_raw(),
                None,
            )?;
            let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);

            // Select physical device
            let physical_devices = instance.enumerate_physical_devices()?;
            let (physical_device, queue_family_index) = physical_devices
                .iter()
                .find_map(|&pd| {
                    let props = instance.get_physical_device_queue_family_properties(pd);
                    props.iter().enumerate().find_map(|(i, prop)| {
                        let supports_graphics = prop.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                        let supports_surface = surface_loader
                            .get_physical_device_surface_support(pd, i as u32, surface)
                            .unwrap_or(false);

                        if supports_graphics && supports_surface {
                            Some((pd, i as u32))
                        } else {
                            None
                        }
                    })
                })
                .ok_or("No suitable physical device found")?;

            let device_props = instance.get_physical_device_properties(physical_device);
            let device_name = CStr::from_ptr(device_props.device_name.as_ptr())
                .to_string_lossy()
                .into_owned();

            let mem_properties = instance.get_physical_device_memory_properties(physical_device);

            // Create device with swapchain extension
            let queue_info = vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family_index)
                .queue_priorities(&[1.0]);

            let device_extensions = vec![
                ash::khr::swapchain::NAME.as_ptr(),
                b"VK_KHR_portability_subset\0".as_ptr() as *const i8,
            ];

            let device_create_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(std::slice::from_ref(&queue_info))
                .enabled_extension_names(&device_extensions);

            let device = instance.create_device(physical_device, &device_create_info, None)?;
            let queue = device.get_device_queue(queue_family_index, 0);

            let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);

            // Create swapchain
            let (swapchain, swapchain_images, swapchain_extent, swapchain_format) =
                Self::create_swapchain(
                    &surface_loader,
                    &swapchain_loader,
                    physical_device,
                    surface,
                    &window,
                    vk::SwapchainKHR::null(),
                )?;

            // Create image views
            let swapchain_image_views = swapchain_images
                .iter()
                .map(|&image| {
                    let create_info = vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(swapchain_format)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        });
                    device.create_image_view(&create_info, None)
                })
                .collect::<Result<Vec<_>, _>>()?;

            // Create render pass
            let attachment = vk::AttachmentDescription::default()
                .format(swapchain_format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

            let color_ref = vk::AttachmentReference::default()
                .attachment(0)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

            let subpass = vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(std::slice::from_ref(&color_ref));

            let dependency = vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

            let render_pass_info = vk::RenderPassCreateInfo::default()
                .attachments(std::slice::from_ref(&attachment))
                .subpasses(std::slice::from_ref(&subpass))
                .dependencies(std::slice::from_ref(&dependency));

            let render_pass = device.create_render_pass(&render_pass_info, None)?;

            // Create framebuffers
            let framebuffers = swapchain_image_views
                .iter()
                .map(|&view| {
                    let attachments = [view];
                    let create_info = vk::FramebufferCreateInfo::default()
                        .render_pass(render_pass)
                        .attachments(&attachments)
                        .width(swapchain_extent.width)
                        .height(swapchain_extent.height)
                        .layers(1);
                    device.create_framebuffer(&create_info, None)
                })
                .collect::<Result<Vec<_>, _>>()?;

            // Create uniform buffer
            let ubo_size = 64;
            let ubo_info = vk::BufferCreateInfo::default()
                .size(ubo_size)
                .usage(vk::BufferUsageFlags::UNIFORM_BUFFER);

            let uniform_buffer = device.create_buffer(&ubo_info, None)?;
            let ubo_req = device.get_buffer_memory_requirements(uniform_buffer);

            let ubo_mem_type = Self::find_memory_type(
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

            // Create texture
            let (texture_image, texture_memory, texture_view) =
                Self::create_texture(&device, &mem_properties)?;

            let sampler_info = vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .address_mode_w(vk::SamplerAddressMode::REPEAT);

            let sampler = device.create_sampler(&sampler_info, None)?;

            // Create descriptor set layout
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
                .pool_sizes(&pool_sizes)
                .max_sets(1);

            let descriptor_pool = device.create_descriptor_pool(&pool_info, None)?;

            // Allocate descriptor set
            let alloc_info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool)
                .set_layouts(std::slice::from_ref(&descriptor_set_layout));

            let descriptor_sets = device.allocate_descriptor_sets(&alloc_info)?;
            let descriptor_set = descriptor_sets[0];

            // Update descriptor set
            let buffer_info = vk::DescriptorBufferInfo::default()
                .buffer(uniform_buffer)
                .offset(0)
                .range(ubo_size);

            let image_info = vk::DescriptorImageInfo::default()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(texture_view)
                .sampler(sampler);

            let descriptor_writes = [
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

            device.update_descriptor_sets(&descriptor_writes, &[]);

            // Create command pool
            let pool_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(queue_family_index)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

            let command_pool = device.create_command_pool(&pool_info, None)?;

            // Create command buffers
            let alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(MAX_FRAMES_IN_FLIGHT as u32);

            let command_buffers = device.allocate_command_buffers(&alloc_info)?;

            // Create sync objects
            let semaphore_info = vk::SemaphoreCreateInfo::default();
            let fence_info = vk::FenceCreateInfo::default()
                .flags(vk::FenceCreateFlags::SIGNALED);

            let mut image_available_semaphores = Vec::new();
            let mut render_finished_semaphores = Vec::new();
            let mut in_flight_fences = Vec::new();

            for _ in 0..MAX_FRAMES_IN_FLIGHT {
                image_available_semaphores.push(device.create_semaphore(&semaphore_info, None)?);
                render_finished_semaphores.push(device.create_semaphore(&semaphore_info, None)?);
                in_flight_fences.push(device.create_fence(&fence_info, None)?);
            }

            Ok(Self {
                entry,
                instance,
                surface,
                surface_loader,
                physical_device,
                device,
                queue,
                queue_family_index,
                swapchain,
                swapchain_loader,
                swapchain_images,
                swapchain_image_views,
                swapchain_extent,
                swapchain_format,
                render_pass,
                framebuffers,
                descriptor_set_layout,
                pipeline_layout,
                pipeline: None,
                uniform_buffer,
                uniform_memory,
                uniform_ptr,
                texture_image,
                texture_memory,
                texture_view,
                sampler,
                descriptor_pool,
                descriptor_set,
                command_pool,
                command_buffers,
                image_available_semaphores,
                render_finished_semaphores,
                in_flight_fences,
                current_frame: 0,
                window,
                device_name,
            })
        }
    }

    fn create_swapchain(
        surface_loader: &ash::khr::surface::Instance,
        swapchain_loader: &ash::khr::swapchain::Device,
        physical_device: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
        window: &Window,
        old_swapchain: vk::SwapchainKHR,
    ) -> Result<(vk::SwapchainKHR, Vec<vk::Image>, vk::Extent2D, vk::Format), Box<dyn std::error::Error>> {
        unsafe {
            let capabilities = surface_loader
                .get_physical_device_surface_capabilities(physical_device, surface)?;

            let formats = surface_loader
                .get_physical_device_surface_formats(physical_device, surface)?;

            let present_modes = surface_loader
                .get_physical_device_surface_present_modes(physical_device, surface)?;

            let surface_format = formats
                .iter()
                .find(|f| {
                    f.format == vk::Format::B8G8R8A8_UNORM
                        && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                })
                .unwrap_or(&formats[0]);

            let present_mode = if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
                vk::PresentModeKHR::MAILBOX
            } else {
                vk::PresentModeKHR::FIFO
            };

            let size = window.inner_size();
            let extent = if capabilities.current_extent.width != u32::MAX {
                capabilities.current_extent
            } else {
                vk::Extent2D {
                    width: size.width.clamp(
                        capabilities.min_image_extent.width,
                        capabilities.max_image_extent.width,
                    ),
                    height: size.height.clamp(
                        capabilities.min_image_extent.height,
                        capabilities.max_image_extent.height,
                    ),
                }
            };

            let image_count = (capabilities.min_image_count + 1).min(
                if capabilities.max_image_count > 0 {
                    capabilities.max_image_count
                } else {
                    u32::MAX
                },
            );

            let create_info = vk::SwapchainCreateInfoKHR::default()
                .surface(surface)
                .min_image_count(image_count)
                .image_format(surface_format.format)
                .image_color_space(surface_format.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(capabilities.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(present_mode)
                .clipped(true)
                .old_swapchain(old_swapchain);

            let swapchain = swapchain_loader.create_swapchain(&create_info, None)?;
            let images = swapchain_loader.get_swapchain_images(swapchain)?;

            Ok((swapchain, images, extent, surface_format.format))
        }
    }

    pub fn load_shader(
        &mut self,
        vert_path: &str,
        frag_path: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            self.device.device_wait_idle()?;

            if let Some(pipeline) = self.pipeline.take() {
                self.device.destroy_pipeline(pipeline, None);
            }

            let vert_code = Self::read_shader_file(vert_path)?;
            let frag_code = Self::read_shader_file(frag_path)?;

            let vert_module = Self::create_shader_module(&self.device, &vert_code)?;
            let frag_module = Self::create_shader_module(&self.device, &frag_code)?;

            let entry_name = std::ffi::CString::new("main").unwrap();

            let vert_stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vert_module)
                .name(&entry_name);

            let frag_stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag_module)
                .name(&entry_name);

            let stages = [vert_stage, frag_stage];

            let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();

            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.swapchain_extent.width as f32,
                height: self.swapchain_extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };

            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.swapchain_extent,
            };

            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewports(std::slice::from_ref(&viewport))
                .scissors(std::slice::from_ref(&scissor));

            let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .line_width(1.0)
                .cull_mode(vk::CullModeFlags::NONE)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE);

            let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);

            let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(false);

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
                &[pipeline_info],
                None,
            ).map_err(|(_, e)| e)?;

            self.pipeline = Some(pipelines[0]);

            self.device.destroy_shader_module(vert_module, None);
            self.device.destroy_shader_module(frag_module, None);

            Ok(())
        }
    }

    pub fn recreate_swapchain(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            self.device.device_wait_idle()?;

            // Destroy old framebuffers
            for &framebuffer in &self.framebuffers {
                self.device.destroy_framebuffer(framebuffer, None);
            }

            // Destroy old image views
            for &view in &self.swapchain_image_views {
                self.device.destroy_image_view(view, None);
            }

            let old_swapchain = self.swapchain;

            // Create new swapchain
            let (swapchain, swapchain_images, swapchain_extent, swapchain_format) =
                Self::create_swapchain(
                    &self.surface_loader,
                    &self.swapchain_loader,
                    self.physical_device,
                    self.surface,
                    &self.window,
                    old_swapchain,
                )?;

            // Destroy old swapchain
            self.swapchain_loader.destroy_swapchain(old_swapchain, None);

            // Update swapchain data
            self.swapchain = swapchain;
            self.swapchain_images = swapchain_images.clone();
            self.swapchain_extent = swapchain_extent;
            self.swapchain_format = swapchain_format;

            // Create new image views
            self.swapchain_image_views = swapchain_images
                .iter()
                .map(|&image| {
                    let create_info = vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(swapchain_format)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        });
                    self.device.create_image_view(&create_info, None)
                })
                .collect::<Result<Vec<_>, _>>()?;

            // Create new framebuffers
            self.framebuffers = self.swapchain_image_views
                .iter()
                .map(|&view| {
                    let attachments = [view];
                    let create_info = vk::FramebufferCreateInfo::default()
                        .render_pass(self.render_pass)
                        .attachments(&attachments)
                        .width(swapchain_extent.width)
                        .height(swapchain_extent.height)
                        .layers(1);
                    self.device.create_framebuffer(&create_info, None)
                })
                .collect::<Result<Vec<_>, _>>()?;

            // Recreate pipeline with new viewport if a shader is loaded
            if self.pipeline.is_some() {
                // Pipeline recreation will be triggered by setting pipeline to None
                // The load_shader function should be called again to recreate with correct viewport
            }

            Ok(())
        }
    }

    pub fn render_frame<T: Copy>(&mut self, ubo_data: &T) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            let fence = self.in_flight_fences[self.current_frame];
            self.device.wait_for_fences(&[fence], true, u64::MAX)?;

            let (image_index, _suboptimal) = match self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.image_available_semaphores[self.current_frame],
                vk::Fence::null(),
            ) {
                Ok(result) => result,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.recreate_swapchain()?;
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            };

            self.device.reset_fences(&[fence])?;

            // Update uniform buffer
            std::ptr::copy_nonoverlapping(
                ubo_data as *const T as *const u8,
                self.uniform_ptr,
                std::mem::size_of::<T>(),
            );

            // Record command buffer
            let cmd_buf = self.command_buffers[self.current_frame];

            self.device.reset_command_buffer(cmd_buf, vk::CommandBufferResetFlags::empty())?;

            let begin_info = vk::CommandBufferBeginInfo::default();
            self.device.begin_command_buffer(cmd_buf, &begin_info)?;

            let clear_color = vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            };

            let render_pass_info = vk::RenderPassBeginInfo::default()
                .render_pass(self.render_pass)
                .framebuffer(self.framebuffers[image_index as usize])
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.swapchain_extent,
                })
                .clear_values(std::slice::from_ref(&clear_color));

            if let Some(pipeline) = self.pipeline {
                self.device.cmd_begin_render_pass(
                    cmd_buf,
                    &render_pass_info,
                    vk::SubpassContents::INLINE,
                );

                self.device.cmd_bind_pipeline(
                    cmd_buf,
                    vk::PipelineBindPoint::GRAPHICS,
                    pipeline,
                );

                self.device.cmd_bind_descriptor_sets(
                    cmd_buf,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline_layout,
                    0,
                    &[self.descriptor_set],
                    &[],
                );

                self.device.cmd_draw(cmd_buf, 6, 1, 0, 0);

                self.device.cmd_end_render_pass(cmd_buf);
            }

            self.device.end_command_buffer(cmd_buf)?;

            // Submit
            let wait_semaphores = [self.image_available_semaphores[self.current_frame]];
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let signal_semaphores = [self.render_finished_semaphores[self.current_frame]];
            let command_buffers = [cmd_buf];

            let submit_info = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&command_buffers)
                .signal_semaphores(&signal_semaphores);

            self.device.queue_submit(self.queue, &[submit_info], fence)?;

            // Present
            let swapchains = [self.swapchain];
            let image_indices = [image_index];

            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);

            match self.swapchain_loader.queue_present(self.queue, &present_info) {
                Ok(_) => {}
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::SUBOPTIMAL_KHR) => {
                    self.recreate_swapchain()?;
                }
                Err(e) => return Err(e.into()),
            }

            self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;

            Ok(())
        }
    }

    pub fn get_device_name(&self) -> &str {
        &self.device_name
    }

    fn read_shader_file(path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    fn create_shader_module(
        device: &ash::Device,
        code: &[u8],
    ) -> Result<vk::ShaderModule, Box<dyn std::error::Error>> {
        unsafe {
            let code_aligned = std::slice::from_raw_parts(
                code.as_ptr() as *const u32,
                code.len() / 4,
            );

            let create_info = vk::ShaderModuleCreateInfo::default().code(code_aligned);

            Ok(device.create_shader_module(&create_info, None)?)
        }
    }

    fn find_memory_type(
        mem_properties: &vk::PhysicalDeviceMemoryProperties,
        type_filter: u32,
        properties: vk::MemoryPropertyFlags,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        for i in 0..mem_properties.memory_type_count {
            if (type_filter & (1 << i)) != 0
                && mem_properties.memory_types[i as usize]
                    .property_flags
                    .contains(properties)
            {
                return Ok(i);
            }
        }
        Err("Failed to find suitable memory type".into())
    }

    fn create_texture(
        device: &ash::Device,
        mem_properties: &vk::PhysicalDeviceMemoryProperties,
    ) -> Result<(vk::Image, vk::DeviceMemory, vk::ImageView), Box<dyn std::error::Error>> {
        unsafe {
            let width = 256u32;
            let height = 256u32;

            let image_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::LINEAR)
                .usage(vk::ImageUsageFlags::SAMPLED)
                .initial_layout(vk::ImageLayout::PREINITIALIZED);

            let image = device.create_image(&image_info, None)?;
            let mem_req = device.get_image_memory_requirements(image);

            let mem_type = Self::find_memory_type(
                mem_properties,
                mem_req.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_req.size)
                .memory_type_index(mem_type);

            let memory = device.allocate_memory(&alloc_info, None)?;
            device.bind_image_memory(image, memory, 0)?;

            // Fill texture with checkerboard pattern
            let ptr = device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())?;
            let pixels = std::slice::from_raw_parts_mut(ptr as *mut u8, (width * height * 4) as usize);

            for y in 0..height {
                for x in 0..width {
                    let idx = ((y * width + x) * 4) as usize;
                    let checker = ((x / 32) + (y / 32)) % 2;
                    let color = if checker == 0 { 255 } else { 128 };
                    pixels[idx] = color;
                    pixels[idx + 1] = color;
                    pixels[idx + 2] = color;
                    pixels[idx + 3] = 255;
                }
            }

            device.unmap_memory(memory);

            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let view = device.create_image_view(&view_info, None)?;

            Ok((image, memory, view))
        }
    }
}

impl Drop for SwapchainRenderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();

            for &semaphore in &self.image_available_semaphores {
                self.device.destroy_semaphore(semaphore, None);
            }
            for &semaphore in &self.render_finished_semaphores {
                self.device.destroy_semaphore(semaphore, None);
            }
            for &fence in &self.in_flight_fences {
                self.device.destroy_fence(fence, None);
            }

            self.device.destroy_command_pool(self.command_pool, None);

            for &framebuffer in &self.framebuffers {
                self.device.destroy_framebuffer(framebuffer, None);
            }

            for &view in &self.swapchain_image_views {
                self.device.destroy_image_view(view, None);
            }

            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);

            if let Some(pipeline) = self.pipeline {
                self.device.destroy_pipeline(pipeline, None);
            }

            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_sampler(self.sampler, None);
            self.device.destroy_image_view(self.texture_view, None);
            self.device.destroy_image(self.texture_image, None);
            self.device.free_memory(self.texture_memory, None);
            self.device.unmap_memory(self.uniform_memory);
            self.device.destroy_buffer(self.uniform_buffer, None);
            self.device.free_memory(self.uniform_memory, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}
