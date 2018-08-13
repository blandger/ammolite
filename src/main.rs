#[macro_use]
extern crate vulkano;
#[macro_use]
extern crate vulkano_shader_derive;
extern crate vulkano_win;
extern crate winit;

use std::sync::Arc;
use std::mem;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer, DeviceLocalBuffer};
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::device::{Device, DeviceExtensions, Queue};
use vulkano::instance::{Instance, PhysicalDevice, QueueFamily, Features};
use vulkano::sync::{FlushError, GpuFuture};
use vulkano::format::FormatTy;
use vulkano::image::{AttachmentImage, ImageUsage};
use vulkano::image::swapchain::SwapchainImage;
use vulkano::framebuffer::{Framebuffer, RenderPass, RenderPassDesc, Subpass};
use vulkano::pipeline::GraphicsPipeline;
use vulkano::pipeline::viewport::Viewport;
use vulkano::pipeline::vertex::SingleBufferDefinition;
use vulkano::descriptor::PipelineLayoutAbstract;
use vulkano::descriptor::descriptor_set::PersistentDescriptorSet;
use vulkano::swapchain::{self, PresentMode, SurfaceTransform, Swapchain, AcquireError, SwapchainCreationError, Surface};
use vulkano::sampler::{Sampler, SamplerAddressMode, BorderColor, MipmapMode, Filter};
use vulkano_win::VkSurfaceBuild;
use winit::{EventsLoop, WindowBuilder, Window};

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 3],
}

impl_vertex!(Vertex, position);

mod screen_vs {
    #[derive(VulkanoShader)]
    #[ty = "vertex"]
    #[path = "src/shaders/screen.vert"]
    #[allow(dead_code)]
    struct Dummy;
}

mod screen_fs {
    #[derive(VulkanoShader)]
    #[ty = "fragment"]
    #[path = "src/shaders/screen.frag"]
    #[allow(dead_code)]
    struct Dummy;
}

mod main_vs {
    #[derive(VulkanoShader)]
    #[ty = "vertex"]
    #[path = "src/shaders/main.vert"]
    #[allow(dead_code)]
    struct Dummy;
}

mod main_fs {
    #[derive(VulkanoShader)]
    #[ty = "fragment"]
    #[path = "src/shaders/main.frag"]
    #[allow(dead_code)]
    struct Dummy;
}

use main_fs::ty::*;

impl MainUBO {
    pub fn new(dimensions: [f32; 2], model: [[f32; 4]; 4], view: [[f32; 4]; 4], projection: [[f32; 4]; 4]) -> MainUBO {
        MainUBO {
            dimensions,
            _dummy0: Default::default(),
            model,
            view,
            projection,
        }
    }
}

const SCREEN_DIMENSIONS: [u32; 2] = [3840, 1080];

fn vulkan_initialize<'a>(instance: &'a Arc<Instance>) -> (EventsLoop, Arc<Surface<Window>>, [u32; 2], Arc<Device>, QueueFamily<'a>, Arc<Queue>, Arc<Swapchain<Window>>, Vec<Arc<SwapchainImage<Window>>>) {
    // TODO: Better device selection & SLI support
    let physical_device = PhysicalDevice::enumerate(instance).next().expect("No physical device available.");

    // Queues are like CPU threads, queue families are groups of queues with certain capabilities.
    println!("Available queue families:");

    let events_loop = EventsLoop::new();
    let window = VkSurfaceBuild::build_vk_surface(WindowBuilder::new(), &events_loop, instance.clone()).unwrap();

    let mut dimensions: [u32; 2] = {
        let (width, height) = window.window().get_inner_size().unwrap().into();
        [width, height]
    };

    /*
     * Device Creation
     */

    for queue_family in physical_device.queue_families() {
        println!("\tFamily #{} -- queues: {}, supports graphics: {}, supports compute: {}, supports transfers: {}, supports sparse binding: {}", queue_family.id(), queue_family.queues_count(), queue_family.supports_graphics(), queue_family.supports_compute(), queue_family.supports_transfers(), queue_family.supports_sparse_binding());
    }

    let queue_family = physical_device.queue_families()
        .find(|&queue| {
            queue.supports_graphics() && window.is_supported(queue).unwrap_or(false)
        })
        .expect("Couldn't find a graphical queue family.");

    // Create a device with a single queue
    let (device, mut queues) = {
        let device_extensions = DeviceExtensions {
            khr_swapchain: true,
            .. DeviceExtensions::none()
        };

        Device::new(physical_device,
                    &Features::none(),
                    &device_extensions,
                    // A list of queues to use specified by an iterator of (QueueFamily, priority).
                    // In a real-life application, we would probably use at least a graphics queue
                    // and a transfers queue to handle data transfers in parallel. In this example
                    // we only use one queue.
                    [(queue_family, 0.5)].iter().cloned())
            .expect("Failed to create a Vulkan device.")
    };
    // println!("queues: {}", queues.len());
    let queue = queues.next().unwrap();

    let (swapchain, images) = {
        let capabilities = window.capabilities(physical_device)
            .expect("Failed to retrieve surface capabilities.");

        // Determines the behaviour of the alpha channel
        let alpha = capabilities.supported_composite_alpha.iter().next().unwrap();
        dimensions = capabilities.current_extent.unwrap_or(dimensions);

        // Choosing the internal format that the images will have.
        let format = capabilities.supported_formats[0].0;

        // Please take a look at the docs for the meaning of the parameters we didn't mention.
        Swapchain::new(device.clone(), window.clone(), capabilities.min_image_count, format,
                       dimensions, 1, capabilities.supported_usage_flags, &queue,
                       SurfaceTransform::Identity, alpha, PresentMode::Fifo, true,
                       None).expect("failed to create swapchain")
    };

    (events_loop, window, dimensions, device, queue_family, queue, swapchain, images)
}

fn vulkan_main_pipeline(device: &Arc<Device>, swapchain: &Arc<Swapchain<Window>>) -> Arc<GraphicsPipeline<SingleBufferDefinition<Vertex>, Box<dyn PipelineLayoutAbstract + Send + Sync>, Arc<RenderPass<impl RenderPassDesc>>>> {
    let main_vs = main_vs::Shader::load(device.clone()).expect("Failed to create shader module.");
    let main_fs = main_fs::Shader::load(device.clone()).expect("Failed to create shader module.");

    // A special GPU mode highly-optimized for rendering
    let render_pass = Arc::new(single_pass_renderpass! { device.clone(),
        attachments: {
            color: {
                load: Clear,
                store: Store, // for temporary images, use DontCare
                format: swapchain.format(),
                samples: 1,
            }
        },
        pass: {
            color: [color],
            depth_stencil: {}
        }
    }.unwrap());

    Arc::new(GraphicsPipeline::start()
        // .with_pipeline_layout(device.clone(), pipeline_layout)
        // Specifies the Vertex type
        .vertex_input_single_buffer::<Vertex>()
        .vertex_shader(main_vs.main_entry_point(), ())
        // Configures the builder so that we use one viewport, and that the state of this viewport
        // is dynamic. This makes it possible to change the viewport for each draw command. If the
        // viewport state wasn't dynamic, then we would have to create a new pipeline object if we
        // wanted to draw to another image of a different size.
        //
        // Note: If you configure multiple viewports, you can use geometry shaders to choose which
        // viewport the shape is going to be drawn to. This topic isn't covered here.
        .viewports_dynamic_scissors_irrelevant(1)
        .fragment_shader(main_fs.main_entry_point(), ())
        .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
        .build(device.clone())
        .unwrap())
}

fn create_staging_buffers_data<T>(device: &Arc<Device>, queue_family: QueueFamily, usage: BufferUsage, data: T)
    -> (Arc<CpuAccessibleBuffer<T>>, Arc<DeviceLocalBuffer<T>>)
    where T: Sized + 'static {
    let staging_buffer = CpuAccessibleBuffer::<T>::from_data(
        device.clone(),
        BufferUsage {
            transfer_destination: true,
            transfer_source: true,
            .. usage.clone()
        },
        data,
    ).unwrap();
    let device_buffer = DeviceLocalBuffer::<T>::new(
        device.clone(),
        BufferUsage {
            transfer_destination: true,
            .. usage.clone()
        },
        vec![queue_family],
    ).unwrap();

    (staging_buffer, device_buffer)
}

fn create_staging_buffers_iter<T, I>(device: &Arc<Device>, queue_family: QueueFamily, usage: BufferUsage, iterator: I)
    -> (Arc<CpuAccessibleBuffer<[T]>>, Arc<DeviceLocalBuffer<[T]>>)
    where T: Clone + 'static,
          I: ExactSizeIterator<Item=T> {
    let iterator_len = iterator.len();
    let staging_buffer = CpuAccessibleBuffer::<[T]>::from_iter(
        device.clone(),
        BufferUsage {
            transfer_destination: true,
            transfer_source: true,
            .. usage.clone()
        },
        iterator,
    ).unwrap();
    let device_buffer = DeviceLocalBuffer::<[T]>::array(
        device.clone(),
        iterator_len,
        BufferUsage {
            transfer_destination: true,
            .. usage.clone()
        },
        vec![queue_family],
    ).unwrap();

    (staging_buffer, device_buffer)
}

fn create_vertex_index_buffers<V, I, VI, II>(device: &Arc<Device>, queue_family: QueueFamily, vertex_iterator: VI, index_iterator: II)
    -> ((Arc<CpuAccessibleBuffer<[V]>>, Arc<DeviceLocalBuffer<[V]>>),
        (Arc<CpuAccessibleBuffer<[I]>>, Arc<DeviceLocalBuffer<[I]>>))
    where V: vulkano::pipeline::vertex::Vertex + Clone + 'static,
          I: vulkano::pipeline::input_assembly::Index + Clone + 'static,
          VI: ExactSizeIterator<Item=V>,
          II: ExactSizeIterator<Item=I> {
    (
        create_staging_buffers_iter(
            &device,
            queue_family,
            BufferUsage::vertex_buffer(),
            vertex_iterator,
        ),
        create_staging_buffers_iter(
            &device,
            queue_family,
            BufferUsage::index_buffer(),
            index_iterator,
        ),
    )
}

fn vulkan_screen_pipeline(device: &Arc<Device>, swapchain: &Arc<Swapchain<Window>>) -> Arc<GraphicsPipeline<SingleBufferDefinition<Vertex>, Box<dyn PipelineLayoutAbstract + Send + Sync>, Arc<RenderPass<impl RenderPassDesc>>>> {
    // A special GPU mode highly-optimized for rendering
    let screen_vs = screen_vs::Shader::load(device.clone()).expect("Failed to create shader module.");
    let screen_fs = screen_fs::Shader::load(device.clone()).expect("Failed to create shader module.");

    let render_pass = Arc::new(single_pass_renderpass! { device.clone(),
        attachments: {
            color: {
                load: Clear,
                store: Store, // for temporary images, use DontCare
                format: swapchain.format(),
                samples: 1,
            }
        },
        pass: {
            color: [color],
            depth_stencil: {}
        }
    }.unwrap());

    // let pipeline_layout = PipelineLayour

    Arc::new(GraphicsPipeline::start()
        // .with_pipeline_layout(device.clone(), pipeline_layout)
        // Specifies the Vertex type
        .vertex_input_single_buffer::<Vertex>()
        .vertex_shader(screen_vs.main_entry_point(), ())
        // Configures the builder so that we use one viewport, and that the state of this viewport
        // is dynamic. This makes it possible to change the viewport for each draw command. If the
        // viewport state wasn't dynamic, then we would have to create a new pipeline object if we
        // wanted to draw to another image of a different size.
        //
        // Note: If you configure multiple viewports, you can use geometry shaders to choose which
        // viewport the shape is going to be drawn to. This topic isn't covered here.
        .viewports(vec![Viewport {
            origin: [0.0, 0.0],
            dimensions: [SCREEN_DIMENSIONS[0] as f32, SCREEN_DIMENSIONS[1] as f32],
            depth_range: 0.0 .. 1.0,
        }])
        .fragment_shader(screen_fs.main_entry_point(), ())
        .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
        .build(device.clone())
        .unwrap())
}

type Mat4 = [[f32; 4]; 4];
type Vec3 = [f32; 3];

macro_rules! mat4 {
    {
        $m00:expr, $m10:expr, $m20:expr, $m30:expr;
        $m01:expr, $m11:expr, $m21:expr, $m31:expr;
        $m02:expr, $m12:expr, $m22:expr, $m32:expr;
        $m03:expr, $m13:expr, $m23:expr, $m33:expr$(;)*
    } => {
        [[$m00, $m01, $m02, $m03],
         [$m10, $m11, $m12, $m13],
         [$m20, $m21, $m22, $m23],
         [$m30, $m31, $m32, $m33]]
    }
}

fn negate_vector(vector: &mut Vec3) {
    for component in vector {
        *component = -*component;
    }
}

fn identity_matrix() -> Mat4 {
    mat4![1.0, 0.0, 0.0, 0.0;
          0.0, 1.0, 0.0, 0.0;
          0.0, 0.0, 1.0, 0.0;
          0.0, 0.0, 0.0, 1.0]
}

fn multiply_matrices(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut result = identity_matrix();

    for result_row in 0..4 {
        for result_column in 0..4 {
            let mut result_cell = 0.0;

            for cell_index in 0..4 {
                result_cell += a[cell_index][result_row] * b[result_column][cell_index];
            }

            result[result_column][result_row] = result_cell;
        }
    }

    result
}

fn scale_matrix(matrix: &mut Mat4, scale: f32) {
    for i in 0..3 {
        matrix[i][i] *= scale;
    }
}

fn translate_matrix(matrix: &mut Mat4, translation: &Vec3) {
    for i in 0..3 {
        matrix[3][i] = translation[i];
    }
}

fn rotate_matrix(matrix: &mut Mat4, euler_angles: &Vec3) {
    let e = euler_angles;

    if e[0] != 0.0 {
        let a = e[0];
        *matrix = multiply_matrices(
            &mat4![1.0,     0.0,      0.0, 0.0;
                   0.0, a.cos(), -a.sin(), 0.0;
                   0.0, a.sin(),  a.cos(), 0.0;
                   0.0,     0.0,      0.0, 1.0],
            matrix
        );
    }

    if e[1] != 0.0 {
        let b = e[1];
        *matrix = multiply_matrices(
            &mat4![ b.cos(), 0.0, b.sin(), 0.0;
                        0.0, 1.0,     0.0, 0.0;
                   -b.sin(), 0.0, b.cos(), 0.0;
                        0.0, 0.0,     0.0, 1.0],
            matrix
        );
    }

    if e[2] != 0.0 {
        let c = e[2];
        *matrix = multiply_matrices(
            &mat4![c.cos(), -c.sin(), 0.0, 0.0;
                   c.sin(),  c.cos(), 0.0, 0.0;
                       0.0,      0.0, 1.0, 0.0;
                       0.0,      0.0, 0.0, 1.0],
            matrix
        );
    }
}

fn construct_model_matrix(scale: f32, translation: &Vec3, rotation: &Vec3) -> Mat4 {
    let mut result = identity_matrix();

    scale_matrix(&mut result, scale);
    rotate_matrix(&mut result, rotation);
    translate_matrix(&mut result, translation);

    result
}

fn construct_view_matrix(camera_translation: &Vec3, camera_rotation: &Vec3) -> Mat4 {
    let mut translation = camera_translation.clone();
    let mut rotation = camera_rotation.clone();

    negate_vector(&mut translation);
    negate_vector(&mut rotation);
    construct_model_matrix(1.0, &translation, &rotation)
}

fn construct_ortographic_projection_matrix(near_plane: f32, far_plane: f32) -> Mat4 {
    mat4![1.0, 0.0,                            0.0,         0.0;
          0.0, 1.0,                            0.0,         0.0;
          0.0, 0.0, 1.0 / (far_plane - near_plane), -near_plane;
          0.0, 0.0,                            0.0,         1.0]
}

fn main() {
    /*
     * Initialization
*/

    // TODO: Explore method arguments
    let extensions = vulkano_win::required_extensions();
    let instance = Instance::new(None, &extensions, None)
        .expect("Failed to create a Vulkan instance.");

    let layers = vulkano::instance::layers_list().unwrap()
        .map(|l| l.name().to_string());

    for layer in layers {
        println!("\t{}", layer);
    }

    let (mut events_loop, window, mut dimensions, device, queue_family, queue, mut swapchain, mut images) = vulkan_initialize(&instance);

    /*
     * Buffer Creation
     *
     * Vulkano does not provide a generic Buffer struct which you could create with Buffer::new.
     * Instead, it provides several different structs that all represent buffers, each of these
     * structs being optimized for a certain kind of usage. For example, if you want to
     * continuously upload data you should use a CpuBufferPool, while on the other hand if you have
     * some data that you are never going to modify you should use an ImmutableBuffer.
*/

    let main_vertices = [
        Vertex { position: [-0.5, -0.5,  0.0] },
        Vertex { position: [ 0.5, -0.5,  0.0] },
        Vertex { position: [ 0.5,  0.5,  0.0] },
        Vertex { position: [-0.5,  0.5,  0.0] },

        Vertex { position: [-0.5, -0.5, -0.5] },
        Vertex { position: [ 0.5, -0.5, -0.5] },
        Vertex { position: [ 0.5,  0.5, -0.5] },
        Vertex { position: [-0.5,  0.5, -0.5] },
    ];

    let main_indices = [
        0, 1, 2,
        2, 3, 0,

        4, 5, 6,
        6, 7, 4u16,
    ];

    let (
        (main_vertex_staging_buffer, main_vertex_device_buffer),
        (main_index_staging_buffer, main_index_device_buffer),
    ) = create_vertex_index_buffers(
        &device,
        queue_family,
        main_vertices.into_iter().cloned(),
        main_indices.into_iter().cloned(),
    );

    let screen_vertices = [
        Vertex { position: [-1.0, -1.0,  0.0] },
        Vertex { position: [-1.0,  1.0,  0.0] },
        Vertex { position: [ 1.0,  1.0,  0.0] },
        Vertex { position: [ 1.0, -1.0,  0.0] },
    ];

    let screen_indices = [
        0, 1, 2u16,
        // 2, 3, 0u16,
    ];

    let (
        (screen_vertex_staging_buffer, screen_vertex_device_buffer),
        (screen_index_staging_buffer, screen_index_device_buffer),
    ) = create_vertex_index_buffers(
        &device,
        queue_family,
        screen_vertices.into_iter().cloned(),
        screen_indices.into_iter().cloned(),
    );

    let main_pipeline = vulkan_main_pipeline(&device, &swapchain);
    let screen_pipeline = vulkan_screen_pipeline(&device, &swapchain);

    // let time_buffer = CpuBufferPool::<Time>::uniform_buffer(device.clone());

    // let descriptor_set = Arc::new(PersistentDescriptorSet::start(pipeline.clone(), 0)
    //     .add_buffer(time_buffer).unwrap()
    //     .build().unwrap()
    // );

    let mut main_ubo = MainUBO::new(
        [dimensions[0] as f32, dimensions[1] as f32],
        identity_matrix(),
        identity_matrix(),
        identity_matrix(),
    );
    let (main_ubo_staging_buffer, main_ubo_device_buffer) = create_staging_buffers_data(
        &device,
        queue_family,
        BufferUsage::uniform_buffer(),
        main_ubo.clone(),
    );

    let screen_image = AttachmentImage::with_usage(
        device.clone(),
        SCREEN_DIMENSIONS.clone(),
        swapchain.format(),
        ImageUsage {
            sampled: true,
            .. ImageUsage::none()
        }
    ).unwrap();
    let border_color = match swapchain.format().ty() {
        FormatTy::Uint | FormatTy::Sint => BorderColor::IntTransparentBlack,
                                      _ => BorderColor::FloatTransparentBlack,
    };
    let screen_sampler = Sampler::new(
        device.clone(),
        Filter::Nearest,  // magnifying filter
        Filter::Linear,  // minifying filter
        MipmapMode::Nearest,
        SamplerAddressMode::ClampToBorder(border_color),
        SamplerAddressMode::ClampToBorder(border_color),
        SamplerAddressMode::ClampToBorder(border_color),
        0.0,  // mip_lod_bias
        // TODO: Turn anisotropic filtering on for better screen readability
        1.0,  // anisotropic filtering (1.0 = off, anything higher = on)
        1.0,  // min_lod
        1.0,  // max_lod
    ).unwrap();
    let main_descriptor_set = Arc::new(
        PersistentDescriptorSet::start(main_pipeline.clone(), 0)
            .add_buffer(main_ubo_device_buffer.clone()).unwrap()
            .add_sampled_image(screen_image.clone(), screen_sampler.clone()).unwrap()
            .build().unwrap()
    );

    let screen_framebuffers: Vec<Arc<Framebuffer<_, _>>> = images.iter().map(|_| {
        Arc::new(
            Framebuffer::start(screen_pipeline.render_pass().clone())
            .add(screen_image.clone()).unwrap()
            .build().unwrap()
        )
    }).collect();
    let mut main_framebuffers: Option<Vec<Arc<Framebuffer<_, _>>>> = None;

    // We need to keep track of whether the swapchain is invalid for the current window,
    // for example when the window is resized.
    let mut recreate_swapchain = false;
    let mut iteration = 0;

    // In the loop below we are going to submit commands to the GPU. Submitting a command produces
    // an object that implements the `GpuFuture` trait, which holds the resources for as long as
    // they are in use by the GPU.
    //
    // Destroying the `GpuFuture` blocks until the GPU is finished executing it. In order to avoid
    // that, we store the submission of the previous frame here.
    let mut previous_frame_end: Box<dyn GpuFuture> = Box::new(vulkano::sync::now(device.clone()));

    loop {
        // It is important to call this function from time to time, otherwise resources will keep
        // accumulating and you will eventually reach an out of memory error.
        // Calling this function polls various fences in order to determine what the GPU has
        // already processed, and frees the resources that are no longer needed.
        previous_frame_end.cleanup_finished();

        if recreate_swapchain {
            dimensions = {
                let (width, height) = window.window().get_inner_size().unwrap().into();
                [width, height]
            };

            main_ubo.dimensions = [dimensions[0] as f32, dimensions[1] as f32];

            let (new_swapchain, new_images) = match swapchain.recreate_with_dimension(dimensions) {
                Ok(r) => r,
                // This error tends to happen when the user is manually resizing the window.
                // Simply restarting the loop is the easiest way to fix this issue.
                Err(SwapchainCreationError::UnsupportedDimensions) => {
                    continue;
                },
                Err(err) => panic!("{:?}", err)
            };

            mem::replace(&mut swapchain, new_swapchain);
            mem::replace(&mut images, new_images);

            main_framebuffers = None;

            recreate_swapchain = false;
        }
        //
        // Because framebuffers contains an Arc on the old swapchain, we need to
        // recreate framebuffers as well.
        if main_framebuffers.is_none() {
            let new_main_framebuffers = Some(images.iter().map(|image| {
                Arc::new(Framebuffer::start(main_pipeline.render_pass().clone())
                         .add(image.clone()).unwrap()
                         .build().unwrap())
            }).collect::<Vec<_>>());
            mem::replace(&mut main_framebuffers, new_main_framebuffers);
        }

        // Before we can draw on the output, we have to *acquire* an image from the swapchain. If
        // no image is available (which happens if you submit draw commands too quickly), then the
        // function will block.
        // This operation returns the index of the image that we are allowed to draw upon.
        //
        // This function can block if no image is available. The parameter is an optional timeout
        // after which the function call will return an error.
        let (image_num, acquire_future) = match swapchain::acquire_next_image(swapchain.clone(),
                                                                              None) {
            Ok((image_num, acquire_future)) => {
                (image_num, acquire_future)
            },
            Err(AcquireError::OutOfDate) => {
                recreate_swapchain = true;
                continue;
            },
            Err(err) => panic!("{:?}", err)
        };

        let mut main_ubo = MainUBO::new(
            [dimensions[0] as f32, dimensions[1] as f32],
            construct_model_matrix(1.0,
                                   &[0.0, 0.0, 10.0],
                                   &[iteration as f32 * 0.01, iteration as f32 * 0.01, 0.0]),
            construct_view_matrix(&[(iteration as f32 * 0.02).cos(), 0.0, 0.0],
                                  &[0.0, 0.0, 0.0]),
            construct_ortographic_projection_matrix(0.0, 1000.0),
        );

        let screen_command_buffer = AutoCommandBufferBuilder::primary_one_time_submit(device.clone(), queue.family()).unwrap()
            .copy_buffer(screen_vertex_staging_buffer.clone(), screen_vertex_device_buffer.clone()).unwrap()
            .copy_buffer(screen_index_staging_buffer.clone(), screen_index_device_buffer.clone()).unwrap()
            .begin_render_pass(screen_framebuffers[image_num].clone(),
                               false,
                               vec![[0.0, 1.0, 0.0, 1.0].into()]).unwrap()
            .draw_indexed(screen_pipeline.clone(),
                  &DynamicState::none(),
                  screen_vertex_device_buffer.clone(),
                  screen_index_device_buffer.clone(),
                  (), ()).unwrap()
            .end_render_pass().unwrap()
            .build().unwrap();

        let command_buffer = AutoCommandBufferBuilder::primary_one_time_submit(device.clone(), queue.family()).unwrap()
            .update_buffer(main_ubo_staging_buffer.clone(), main_ubo.clone()).unwrap()
            .copy_buffer(main_ubo_staging_buffer.clone(), main_ubo_device_buffer.clone()).unwrap()
            .copy_buffer(main_vertex_staging_buffer.clone(), main_vertex_device_buffer.clone()).unwrap()
            .copy_buffer(main_index_staging_buffer.clone(), main_index_device_buffer.clone()).unwrap()
            // Before we can draw, we have to *enter a render pass*. There are two methods to do
            // this: `draw_inline` and `draw_secondary`. The latter is a bit more advanced and is
            // not covered here.
            //
            // The third parameter builds the list of values to clear the attachments with. The API
            // is similar to the list of attachments when building the framebuffers, except that
            // only the attachments that use `load: Clear` appear in the list.
            .begin_render_pass(main_framebuffers.as_ref().unwrap()[image_num].clone(), false,
                               vec![[0.0, 0.0, 1.0, 1.0].into()])
            .unwrap()
            // We are now inside the first subpass of the render pass. We add a draw command.
            //
            // The last two parameters contain the list of resources to pass to the shaders.
            // Since we used an `EmptyPipeline` object, the objects have to be `()`.
            .draw_indexed(main_pipeline.clone(),
                  &DynamicState {
                      line_width: None,
                      // TODO: Find a way to do this without having to dynamically allocate a Vec every frame.
                      viewports: Some(vec![Viewport {
                          origin: [0.0, 0.0],
                          dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                          depth_range: 0.0 .. 1.0,
                      }]),
                      scissors: None,
                  },
                  main_vertex_device_buffer.clone(),
                  main_index_device_buffer.clone(),
                  main_descriptor_set.clone(),
                  ())
            .unwrap()
            // We leave the render pass by calling `draw_end`. Note that if we had multiple
            // subpasses we could have called `next_inline` (or `next_secondary`) to jump to the
            // next subpass.
            .end_render_pass()
            .unwrap()
            // Finish building the command buffer by calling `build`.
            .build().unwrap();

        let result = previous_frame_end.join(acquire_future)
            .then_execute(queue.clone(), screen_command_buffer).unwrap()
            .then_signal_fence()
            .then_execute_same_queue(command_buffer).unwrap()

            // The color output is now expected to contain our triangle. But in order to show it on
            // the screen, we have to *present* the image by calling `present`.
            //
            // This function does not actually present the image immediately. Instead it submits a
            // present command at the end of the queue. This means that it will only be presented once
            // the GPU has finished executing the command buffer that draws the triangle.
            .then_swapchain_present(queue.clone(), swapchain.clone(), image_num)
            .then_signal_fence_and_flush();

        previous_frame_end = match result {
            Ok(future) => Box::new(future) as Box<GpuFuture>,
            Err(FlushError::OutOfDate) => {
                recreate_swapchain = true;
                Box::new(vulkano::sync::now(device.clone())) as Box<GpuFuture>
            },
            Err(err) => panic!("{:?}", err),
        };

        if recreate_swapchain {
            continue;
        }

        // Note that in more complex programs it is likely that one of `acquire_next_image`,
        // `command_buffer::submit`, or `present` will block for some time. This happens when the
        // GPU's queue is full and the driver has to wait until the GPU finished some work.
        //
        // Unfortunately the Vulkan API doesn't provide any way to not wait or to detect when a
        // wait would happen. Blocking may be the desired behavior, but if you don't want to
        // block you should spawn a separate thread dedicated to submissions.

        // Handling the window events in order to close the program when the user wants to close
        // it.
        let mut done = false;
        events_loop.poll_events(|ev| {
            match ev {
                winit::Event::WindowEvent { event: winit::WindowEvent::CloseRequested, .. } => done = true,
                winit::Event::WindowEvent { event: winit::WindowEvent::Resized(_), .. } => recreate_swapchain = true,
                _ => ()
            }
        });
        if done { return; }
        iteration += 1;
    }
}
