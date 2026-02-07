/* Metalshader - Interactive shader viewer with keyboard navigation
 * Usage: ./metalshader <shader_name>
 *
 * Controls:
 *   Arrow Left/Right: Switch between shaders
 *   ESC/Q: Quit
 *
 * Provides:
 * - binding 0: UniformBufferObject (iResolution, iTime, iMouse)
 * - binding 1: sampler2D (256x256 procedural checkerboard texture)
 */
#define _POSIX_C_SOURCE 199309L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <time.h>
#include <fcntl.h>
#include <unistd.h>
#include <dirent.h>
#include <sys/stat.h>
#include <linux/input.h>
#include <xf86drm.h>
#include <xf86drmMode.h>
#include <gbm.h>
#include <vulkan/vulkan.h>

#define MAX_SHADERS 256

typedef struct {
    float iResolution[3];
    float iTime;
    float iMouse[4];
} ShaderToyUBO;

typedef struct {
    char name[256];  // Base name without extension
    char vert_path[512];
    char frag_path[512];
} ShaderInfo;

static ShaderInfo shaders[MAX_SHADERS];
static int shader_count = 0;
static int current_shader = 0;
static int reload_requested = 0;

static uint32_t *load_spv(const char *p, size_t *sz) {
    FILE *f=fopen(p,"rb"); if(!f) return NULL;
    fseek(f,0,SEEK_END); *sz=ftell(f); fseek(f,0,SEEK_SET);
    uint32_t *d=malloc(*sz); fread(d,1,*sz,f); fclose(f); return d;
}

static uint32_t find_mem(VkPhysicalDeviceMemoryProperties *p, uint32_t bits, VkMemoryPropertyFlags flags) {
    for(uint32_t i=0; i<p->memoryTypeCount; i++)
        if((bits&(1<<i)) && (p->memoryTypes[i].propertyFlags&flags)==flags) return i;
    return UINT32_MAX;
}

#define VK_CHECK(x) do{VkResult r=(x);if(r){printf("VK err %d @ %d\n",r,__LINE__);exit(1);}}while(0)

static void generate_texture(uint8_t *data) {
    for (int y = 0; y < 256; y++) {
        for (int x = 0; x < 256; x++) {
            int idx = (y * 256 + x) * 4;
            int checker = ((x/32) + (y/32)) % 2;
            data[idx+0] = checker ? 200 : 50;
            data[idx+1] = checker ? 180 : 60;
            data[idx+2] = checker ? 160 : 80;
            data[idx+3] = 255;
        }
    }
}

// Extract basename from path (e.g., "shaders/plasma" -> "plasma")
static const char *get_basename(const char *path) {
    const char *last_slash = strrchr(path, '/');
    return last_slash ? last_slash + 1 : path;
}

// Find QEMU display control port dynamically
static const char *find_display_port() {
    static char port_path[64] = {0};
    if (port_path[0]) return port_path;  // Cached

    // Search for org.qemu.display port
    for (int i = 0; i < 10; i++) {
        char name_path[128];
        snprintf(name_path, sizeof(name_path), "/sys/class/virtio-ports/vport%dp1/name", i);
        FILE *f = fopen(name_path, "r");
        if (f) {
            char name[64];
            if (fgets(name, sizeof(name), f)) {
                if (strstr(name, "org.qemu.display")) {
                    snprintf(port_path, sizeof(port_path), "/dev/vport%dp1", i);
                    fclose(f);
                    return port_path;
                }
            }
            fclose(f);
        }
    }
    return NULL;
}

// Scan shaders directory and build list
static void scan_shaders(const char *shader_dir) {
    DIR *dir = opendir(shader_dir);
    if (!dir) {
        return;
    }

    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL && shader_count < MAX_SHADERS) {
        if (entry->d_type != DT_REG) continue;

        char *ext = strrchr(entry->d_name, '.');
        if (!ext || strcmp(ext, ".frag") != 0) continue;

        // Extract base name (remove .frag extension)
        size_t base_len = ext - entry->d_name;
        if (base_len >= sizeof(shaders[0].name)) continue;

        strncpy(shaders[shader_count].name, entry->d_name, base_len);
        shaders[shader_count].name[base_len] = '\0';

        // Build paths
        snprintf(shaders[shader_count].vert_path, sizeof(shaders[0].vert_path),
                 "%s/%s.vert.spv", shader_dir, shaders[shader_count].name);
        snprintf(shaders[shader_count].frag_path, sizeof(shaders[0].frag_path),
                 "%s/%s.frag.spv", shader_dir, shaders[shader_count].name);

        // Check if compiled shaders exist
        struct stat st;
        if (stat(shaders[shader_count].vert_path, &st) == 0 &&
            stat(shaders[shader_count].frag_path, &st) == 0) {
            shader_count++;
        }
    }
    closedir(dir);
}

// Scan multiple directories for shaders
static void scan_all_shaders() {
    const char *search_dirs[] = {
        ".",                           // Current directory
        "./shaders",                   // ./shaders subdirectory
        "/root/metalshade/shaders",    // Default metalshade location
        NULL
    };

    shader_count = 0;
    for (int i = 0; search_dirs[i] != NULL; i++) {
        scan_shaders(search_dirs[i]);
    }

    printf("Found %d compiled shader(s)\n", shader_count);
    for (int i = 0; i < shader_count; i++) {
        printf("  [%d] %s\n", i, shaders[i].name);
    }
}

// Find shader by name
static int find_shader_by_name(const char *name) {
    for (int i = 0; i < shader_count; i++) {
        if (strcmp(shaders[i].name, name) == 0) return i;
    }
    return -1;
}

// Open keyboard input device
static int open_keyboard() {
    for (int i = 0; i < 10; i++) {
        char path[64];
        snprintf(path, sizeof(path), "/dev/input/event%d", i);
        int fd = open(path, O_RDONLY | O_NONBLOCK);
        if (fd >= 0) {
            // Try to grab it (ignore failure, not critical)
            char name[256] = {0};
            ioctl(fd, EVIOCGNAME(sizeof(name)), name);
            if (strstr(name, "keyboard") || strstr(name, "Keyboard")) {
                printf("Using input: %s (%s)\n", path, name);
                return fd;
            }
            close(fd);
        }
    }
    return -1;
}

// Check for keyboard events (non-blocking)
static void check_keyboard(int kbd_fd) {
    if (kbd_fd < 0) return;

    struct input_event ev;
    while (read(kbd_fd, &ev, sizeof(ev)) == sizeof(ev)) {
        if (ev.type != EV_KEY || ev.value != 1) continue;  // Only key press

        switch (ev.code) {
            case KEY_LEFT:
                current_shader = (current_shader - 1 + shader_count) % shader_count;
                reload_requested = 1;
                printf("\n<< Previous shader: %s\n", shaders[current_shader].name);
                break;
            case KEY_RIGHT:
                current_shader = (current_shader + 1) % shader_count;
                reload_requested = 1;
                printf("\n>> Next shader: %s\n", shaders[current_shader].name);
                break;
            case KEY_F:
                // Signal host to toggle fullscreen via virtio-serial
                printf("\n[F] Toggling host fullscreen...\n");
                const char *port = find_display_port();
                if (port) {
                    FILE *f = fopen(port, "w");
                    if (f) {
                        fprintf(f, "FULLSCREEN\n");
                        fflush(f);
                        fclose(f);
                    } else {
                        printf("    (Can't open %s, press Ctrl+Alt+F on Mac host)\n", port);
                    }
                } else {
                    printf("    (No display port found, press Ctrl+Alt+F on Mac host)\n");
                }
                break;
            case KEY_ESC:
            case KEY_Q:
                printf("\nExiting...\n");
                exit(0);
                break;
        }
    }
}

int main(int argc, char **argv) {
    // Default to "example" shader if no argument provided
    const char *shader_arg = (argc < 2) ? "example" : argv[1];

    // Extract basename from shader argument (handles "shaders/plasma" -> "plasma")
    const char *shader_name = get_basename(shader_arg);

    // Scan available shaders in multiple directories
    scan_all_shaders();
    if (shader_count == 0) {
        printf("No compiled shaders found.\n");
        printf("Searched: . ./shaders /root/metalshade/shaders\n");
        printf("Compile shaders with: glslangValidator -V <shader>.vert -o <shader>.vert.spv\n");
        return 1;
    }

    // Find requested shader
    current_shader = find_shader_by_name(shader_name);
    if (current_shader < 0) {
        printf("Shader '%s' not found. Available shaders:\n", shader_name);
        for (int i = 0; i < shader_count; i++) {
            printf("  %s\n", shaders[i].name);
        }
        return 1;
    }

    printf("Starting with shader: %s\n", shaders[current_shader].name);

    // Open keyboard
    int kbd_fd = open_keyboard();
    if (kbd_fd < 0) {
        printf("Warning: No keyboard input found, arrow key navigation disabled\n");
    }

    // DRM/GBM Setup
    int drm_fd = open("/dev/dri/card0", O_RDWR);
    drmSetMaster(drm_fd);
    drmModeRes *res = drmModeGetResources(drm_fd);
    drmModeConnector *conn = NULL;
    for(int i=0; i<res->count_connectors; i++) {
        conn = drmModeGetConnector(drm_fd, res->connectors[i]);
        if(conn && conn->connection == DRM_MODE_CONNECTED) break;
        drmModeFreeConnector(conn); conn = NULL;
    }
    drmModeModeInfo *mode = &conn->modes[0];
    uint32_t W = mode->hdisplay, H = mode->vdisplay;
    drmModeEncoder *enc = drmModeGetEncoder(drm_fd, conn->encoder_id);
    uint32_t crtc_id = enc ? enc->crtc_id : res->crtcs[0];

    struct gbm_device *gbm = gbm_create_device(drm_fd);
    struct gbm_bo *bo = gbm_bo_create(gbm, W, H, GBM_FORMAT_XRGB8888,
                                       GBM_BO_USE_SCANOUT | GBM_BO_USE_RENDERING);
    uint32_t stride = gbm_bo_get_stride(bo);
    uint32_t handles[4] = {gbm_bo_get_handle(bo).u32, 0, 0, 0};
    uint32_t strides[4] = {stride, 0, 0, 0};
    uint32_t offsets[4] = {0, 0, 0, 0};
    uint32_t fb_id;
    drmModeAddFB2(drm_fd, W, H, GBM_FORMAT_XRGB8888, handles, strides, offsets, &fb_id, 0);

    // Vulkan Setup
    VkInstance instance;
    VK_CHECK(vkCreateInstance(&(VkInstanceCreateInfo){
        .sType=VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO}, NULL, &instance));

    uint32_t gpuCount = 1; VkPhysicalDevice gpu;
    vkEnumeratePhysicalDevices(instance, &gpuCount, &gpu);
    VkPhysicalDeviceProperties props;
    vkGetPhysicalDeviceProperties(gpu, &props);
    VkPhysicalDeviceMemoryProperties memProps;
    vkGetPhysicalDeviceMemoryProperties(gpu, &memProps);
    printf("Metalshader on %s (%ux%u)\n", props.deviceName, W, H);

    VkDevice device;
    VK_CHECK(vkCreateDevice(gpu, &(VkDeviceCreateInfo){
        .sType=VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
        .queueCreateInfoCount=1,
        .pQueueCreateInfos=&(VkDeviceQueueCreateInfo){
            .sType=VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
            .queueCount=1,.pQueuePriorities=&(float){1.0f}
        }
    }, NULL, &device));

    VkQueue queue;
    vkGetDeviceQueue(device, 0, 0, &queue);

    // Render target image (LINEAR + HOST_VISIBLE)
    VkImage rtImg;
    VK_CHECK(vkCreateImage(device, &(VkImageCreateInfo){
        .sType=VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
        .imageType=VK_IMAGE_TYPE_2D,.format=VK_FORMAT_B8G8R8A8_UNORM,
        .extent={W,H,1},.mipLevels=1,.arrayLayers=1,
        .samples=VK_SAMPLE_COUNT_1_BIT,.tiling=VK_IMAGE_TILING_LINEAR,
        .usage=VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT
    }, NULL, &rtImg));
    VkMemoryRequirements rtReq;
    vkGetImageMemoryRequirements(device, rtImg, &rtReq);
    VkDeviceMemory rtMem;
    VK_CHECK(vkAllocateMemory(device, &(VkMemoryAllocateInfo){
        .sType=VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize=rtReq.size,
        .memoryTypeIndex=find_mem(&memProps, rtReq.memoryTypeBits,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT|VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)
    }, NULL, &rtMem));
    VK_CHECK(vkBindImageMemory(device, rtImg, rtMem, 0));
    void *rtPtr; VK_CHECK(vkMapMemory(device, rtMem, 0, VK_WHOLE_SIZE, 0, &rtPtr));

    VkImageView rtView;
    VK_CHECK(vkCreateImageView(device, &(VkImageViewCreateInfo){
        .sType=VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
        .image=rtImg,.viewType=VK_IMAGE_VIEW_TYPE_2D,
        .format=VK_FORMAT_B8G8R8A8_UNORM,
        .subresourceRange={VK_IMAGE_ASPECT_COLOR_BIT,0,1,0,1}
    }, NULL, &rtView));

    // Create texture
    VkImage texImg;
    VK_CHECK(vkCreateImage(device, &(VkImageCreateInfo){
        .sType=VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
        .imageType=VK_IMAGE_TYPE_2D,.format=VK_FORMAT_R8G8B8A8_UNORM,
        .extent={256,256,1},.mipLevels=1,.arrayLayers=1,
        .samples=VK_SAMPLE_COUNT_1_BIT,.tiling=VK_IMAGE_TILING_LINEAR,
        .usage=VK_IMAGE_USAGE_SAMPLED_BIT,.initialLayout=VK_IMAGE_LAYOUT_PREINITIALIZED
    }, NULL, &texImg));
    VkMemoryRequirements texReq;
    vkGetImageMemoryRequirements(device, texImg, &texReq);
    VkDeviceMemory texMem;
    VK_CHECK(vkAllocateMemory(device, &(VkMemoryAllocateInfo){
        .sType=VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize=texReq.size,
        .memoryTypeIndex=find_mem(&memProps, texReq.memoryTypeBits,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT|VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)
    }, NULL, &texMem));
    VK_CHECK(vkBindImageMemory(device, texImg, texMem, 0));

    // Upload texture data
    void *texPtr; VK_CHECK(vkMapMemory(device, texMem, 0, VK_WHOLE_SIZE, 0, &texPtr));
    VkSubresourceLayout texLayout;
    vkGetImageSubresourceLayout(device, texImg, &(VkImageSubresource){
        VK_IMAGE_ASPECT_COLOR_BIT,0,0}, &texLayout);
    uint8_t *texData = malloc(256*256*4);
    generate_texture(texData);
    for (int y = 0; y < 256; y++)
        memcpy((char*)texPtr + y * texLayout.rowPitch, texData + y * 256 * 4, 256 * 4);
    free(texData);
    vkUnmapMemory(device, texMem);

    VkImageView texView;
    VK_CHECK(vkCreateImageView(device, &(VkImageViewCreateInfo){
        .sType=VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
        .image=texImg,.viewType=VK_IMAGE_VIEW_TYPE_2D,
        .format=VK_FORMAT_R8G8B8A8_UNORM,
        .subresourceRange={VK_IMAGE_ASPECT_COLOR_BIT,0,1,0,1}
    }, NULL, &texView));

    VkSampler sampler;
    VK_CHECK(vkCreateSampler(device, &(VkSamplerCreateInfo){
        .sType=VK_STRUCTURE_TYPE_SAMPLER_CREATE_INFO,
        .magFilter=VK_FILTER_LINEAR,.minFilter=VK_FILTER_LINEAR,
        .addressModeU=VK_SAMPLER_ADDRESS_MODE_REPEAT,
        .addressModeV=VK_SAMPLER_ADDRESS_MODE_REPEAT,
        .addressModeW=VK_SAMPLER_ADDRESS_MODE_REPEAT
    }, NULL, &sampler));

    // Render pass
    VkRenderPass renderPass;
    VK_CHECK(vkCreateRenderPass(device, &(VkRenderPassCreateInfo){
        .sType=VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO,
        .attachmentCount=1,
        .pAttachments=&(VkAttachmentDescription){
            .format=VK_FORMAT_B8G8R8A8_UNORM,.samples=VK_SAMPLE_COUNT_1_BIT,
            .loadOp=VK_ATTACHMENT_LOAD_OP_CLEAR,.storeOp=VK_ATTACHMENT_STORE_OP_STORE,
            .initialLayout=VK_IMAGE_LAYOUT_UNDEFINED,
            .finalLayout=VK_IMAGE_LAYOUT_GENERAL
        },
        .subpassCount=1,
        .pSubpasses=&(VkSubpassDescription){
            .pipelineBindPoint=VK_PIPELINE_BIND_POINT_GRAPHICS,
            .colorAttachmentCount=1,
            .pColorAttachments=&(VkAttachmentReference){0,VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL}
        }
    }, NULL, &renderPass));

    VkFramebuffer framebuffer;
    VK_CHECK(vkCreateFramebuffer(device, &(VkFramebufferCreateInfo){
        .sType=VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO,
        .renderPass=renderPass,.attachmentCount=1,.pAttachments=&rtView,
        .width=W,.height=H,.layers=1
    }, NULL, &framebuffer));

    // Uniform buffer
    VkBuffer uboBuf;
    VK_CHECK(vkCreateBuffer(device, &(VkBufferCreateInfo){
        .sType=VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        .size=64,.usage=VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT
    }, NULL, &uboBuf));
    VkMemoryRequirements uboReq;
    vkGetBufferMemoryRequirements(device, uboBuf, &uboReq);
    VkDeviceMemory uboMem;
    VK_CHECK(vkAllocateMemory(device, &(VkMemoryAllocateInfo){
        .sType=VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize=uboReq.size,
        .memoryTypeIndex=find_mem(&memProps, uboReq.memoryTypeBits,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT|VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)
    }, NULL, &uboMem));
    VK_CHECK(vkBindBufferMemory(device, uboBuf, uboMem, 0));
    void *uboPtr; vkMapMemory(device, uboMem, 0, 64, 0, &uboPtr);

    // Descriptor setup
    VkDescriptorSetLayoutBinding bindings[2] = {
        {0, VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER, 1, VK_SHADER_STAGE_VERTEX_BIT|VK_SHADER_STAGE_FRAGMENT_BIT},
        {1, VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER, 1, VK_SHADER_STAGE_FRAGMENT_BIT}
    };
    VkDescriptorSetLayout descLayout;
    VK_CHECK(vkCreateDescriptorSetLayout(device, &(VkDescriptorSetLayoutCreateInfo){
        .sType=VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
        .bindingCount=2,.pBindings=bindings
    }, NULL, &descLayout));

    VkPipelineLayout pipelineLayout;
    VK_CHECK(vkCreatePipelineLayout(device, &(VkPipelineLayoutCreateInfo){
        .sType=VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
        .setLayoutCount=1,.pSetLayouts=&descLayout
    }, NULL, &pipelineLayout));

    // Descriptor pool
    VkDescriptorPoolSize poolSizes[] = {
        {VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER, 1},
        {VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER, 1}
    };
    VkDescriptorPool descPool;
    VK_CHECK(vkCreateDescriptorPool(device, &(VkDescriptorPoolCreateInfo){
        .sType=VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
        .maxSets=1,.poolSizeCount=2,.pPoolSizes=poolSizes
    }, NULL, &descPool));
    VkDescriptorSet descSet;
    VK_CHECK(vkAllocateDescriptorSets(device, &(VkDescriptorSetAllocateInfo){
        .sType=VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
        .descriptorPool=descPool,.descriptorSetCount=1,.pSetLayouts=&descLayout
    }, &descSet));

    VkWriteDescriptorSet writes[2] = {
        {.sType=VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
         .dstSet=descSet,.dstBinding=0,.descriptorCount=1,
         .descriptorType=VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,
         .pBufferInfo=&(VkDescriptorBufferInfo){uboBuf,0,64}},
        {.sType=VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
         .dstSet=descSet,.dstBinding=1,.descriptorCount=1,
         .descriptorType=VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
         .pImageInfo=&(VkDescriptorImageInfo){sampler,texView,VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL}}
    };
    vkUpdateDescriptorSets(device, 2, writes, 0, NULL);

    // Command pool
    VkCommandPool cmdPool;
    VK_CHECK(vkCreateCommandPool(device, &(VkCommandPoolCreateInfo){
        .sType=VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO}, NULL, &cmdPool));
    VkCommandBuffer cmd;
    VK_CHECK(vkAllocateCommandBuffers(device, &(VkCommandBufferAllocateInfo){
        .sType=VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
        .commandPool=cmdPool,.level=VK_COMMAND_BUFFER_LEVEL_PRIMARY,
        .commandBufferCount=1
    }, &cmd));

    // Transition texture
    vkBeginCommandBuffer(cmd, &(VkCommandBufferBeginInfo){
        .sType=VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO});
    vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT, 0,
        0, NULL, 0, NULL, 1, &(VkImageMemoryBarrier){
            .sType=VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
            .srcAccessMask=VK_ACCESS_HOST_WRITE_BIT,
            .dstAccessMask=VK_ACCESS_SHADER_READ_BIT,
            .oldLayout=VK_IMAGE_LAYOUT_PREINITIALIZED,
            .newLayout=VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
            .image=texImg,
            .subresourceRange={VK_IMAGE_ASPECT_COLOR_BIT,0,1,0,1}
        });
    vkEndCommandBuffer(cmd);
    VkFence fence;
    VK_CHECK(vkCreateFence(device, &(VkFenceCreateInfo){
        .sType=VK_STRUCTURE_TYPE_FENCE_CREATE_INFO}, NULL, &fence));
    VK_CHECK(vkQueueSubmit(queue, 1, &(VkSubmitInfo){
        .sType=VK_STRUCTURE_TYPE_SUBMIT_INFO,
        .commandBufferCount=1,.pCommandBuffers=&cmd
    }, fence));
    VK_CHECK(vkWaitForFences(device, 1, &fence, VK_TRUE, UINT64_MAX));
    vkResetFences(device, 1, &fence);

    // Get render target layout
    VkSubresourceLayout rtLayout;
    vkGetImageSubresourceLayout(device, rtImg, &(VkImageSubresource){
        VK_IMAGE_ASPECT_COLOR_BIT,0,0}, &rtLayout);

    drmModeSetCrtc(drm_fd, crtc_id, fb_id, 0, 0, &conn->connector_id, 1, mode);

    // Load initial shader
    VkPipeline pipeline = VK_NULL_HANDLE;
    VkShaderModule vm = VK_NULL_HANDLE, fm = VK_NULL_HANDLE;

    // Main loop
    struct timespec start, now;
    clock_gettime(CLOCK_MONOTONIC, &start);
    int frames = 0;

    while(1) {
        // Check for shader reload
        if (reload_requested || pipeline == VK_NULL_HANDLE) {
            // Clean up old shaders/pipeline
            if (pipeline != VK_NULL_HANDLE) vkDestroyPipeline(device, pipeline, NULL);
            if (vm != VK_NULL_HANDLE) vkDestroyShaderModule(device, vm, NULL);
            if (fm != VK_NULL_HANDLE) vkDestroyShaderModule(device, fm, NULL);

            // Load new shaders
            size_t vsz, fsz;
            uint32_t *vc = load_spv(shaders[current_shader].vert_path, &vsz);
            uint32_t *fc = load_spv(shaders[current_shader].frag_path, &fsz);
            if (!vc || !fc) {
                printf("Failed to load shaders for '%s'\n", shaders[current_shader].name);
                sleep(1);
                continue;
            }

            VK_CHECK(vkCreateShaderModule(device, &(VkShaderModuleCreateInfo){
                .sType=VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,.codeSize=vsz,.pCode=vc}, NULL, &vm));
            VK_CHECK(vkCreateShaderModule(device, &(VkShaderModuleCreateInfo){
                .sType=VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,.codeSize=fsz,.pCode=fc}, NULL, &fm));
            free(vc); free(fc);

            // Create pipeline
            VK_CHECK(vkCreateGraphicsPipelines(device, VK_NULL_HANDLE, 1, &(VkGraphicsPipelineCreateInfo){
                .sType=VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
                .stageCount=2,
                .pStages=(VkPipelineShaderStageCreateInfo[]){
                    {.sType=VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
                     .stage=VK_SHADER_STAGE_VERTEX_BIT,.module=vm,.pName="main"},
                    {.sType=VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
                     .stage=VK_SHADER_STAGE_FRAGMENT_BIT,.module=fm,.pName="main"}
                },
                .pVertexInputState=&(VkPipelineVertexInputStateCreateInfo){
                    .sType=VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO
                },
                .pInputAssemblyState=&(VkPipelineInputAssemblyStateCreateInfo){
                    .sType=VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
                    .topology=VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST
                },
                .pViewportState=&(VkPipelineViewportStateCreateInfo){
                    .sType=VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO,
                    .viewportCount=1,.pViewports=&(VkViewport){0,0,W,H,0,1},
                    .scissorCount=1,.pScissors=&(VkRect2D){{0,0},{W,H}}
                },
                .pRasterizationState=&(VkPipelineRasterizationStateCreateInfo){
                    .sType=VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
                    .polygonMode=VK_POLYGON_MODE_FILL,.cullMode=VK_CULL_MODE_NONE,.lineWidth=1.0f
                },
                .pMultisampleState=&(VkPipelineMultisampleStateCreateInfo){
                    .sType=VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
                    .rasterizationSamples=VK_SAMPLE_COUNT_1_BIT
                },
                .pColorBlendState=&(VkPipelineColorBlendStateCreateInfo){
                    .sType=VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
                    .attachmentCount=1,
                    .pAttachments=&(VkPipelineColorBlendAttachmentState){.colorWriteMask=0xF}
                },
                .layout=pipelineLayout,.renderPass=renderPass
            }, NULL, &pipeline));

            printf("Loaded shader: %s\n", shaders[current_shader].name);
            reload_requested = 0;
            clock_gettime(CLOCK_MONOTONIC, &start);
            frames = 0;
        }

        clock_gettime(CLOCK_MONOTONIC, &now);
        float t = (now.tv_sec - start.tv_sec) + (now.tv_nsec - start.tv_nsec) / 1e9f;

        // Check keyboard
        check_keyboard(kbd_fd);

        // Update UBO
        ShaderToyUBO ubo = {
            .iResolution = {W, H, 1.0f},
            .iTime = t,
            .iMouse = {0, 0, 0, 0}
        };
        memcpy(uboPtr, &ubo, sizeof(ubo));

        // Record
        vkBeginCommandBuffer(cmd, &(VkCommandBufferBeginInfo){
            .sType=VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
            .flags=VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT});
        vkCmdBeginRenderPass(cmd, &(VkRenderPassBeginInfo){
            .sType=VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO,
            .renderPass=renderPass,.framebuffer=framebuffer,
            .renderArea={{0,0},{W,H}},.clearValueCount=1,
            .pClearValues=&(VkClearValue){.color={.float32={0,0,0,1}}}
        }, VK_SUBPASS_CONTENTS_INLINE);
        vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
        vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS,
                                pipelineLayout, 0, 1, &descSet, 0, NULL);
        vkCmdDraw(cmd, 6, 1, 0, 0);
        vkCmdEndRenderPass(cmd);
        vkEndCommandBuffer(cmd);

        // Submit and wait
        VK_CHECK(vkQueueSubmit(queue, 1, &(VkSubmitInfo){
            .sType=VK_STRUCTURE_TYPE_SUBMIT_INFO,
            .commandBufferCount=1,.pCommandBuffers=&cmd
        }, fence));
        VK_CHECK(vkWaitForFences(device, 1, &fence, VK_TRUE, UINT64_MAX));
        vkResetFences(device, 1, &fence);

        // Copy to GBM
        void *gbmPtr = NULL; uint32_t gbmStride; void *mapData = NULL;
        gbmPtr = gbm_bo_map(bo, 0, 0, W, H, GBM_BO_TRANSFER_WRITE, &gbmStride, &mapData);
        if (gbmPtr) {
            for (uint32_t y = 0; y < H; y++)
                memcpy((char*)gbmPtr + y * gbmStride,
                       (char*)rtPtr + y * rtLayout.rowPitch, W * 4);
            gbm_bo_unmap(bo, mapData);
        }
        drmModeDirtyFB(drm_fd, fb_id, NULL, 0);

        frames++;
        if (frames % 60 == 0) printf("%.1fs: %d frames (%.1f FPS) - %s\n",
                                      t, frames, frames/t, shaders[current_shader].name);
    }

    return 0;
}
