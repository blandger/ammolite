pub mod error;

use std::sync::Arc;
use std::path::Path;
use std::ops::Deref;
use std::mem;
use std::fmt;
use std::marker::PhantomData;
use vulkano;
use vulkano::sync::GpuFuture;
use vulkano::command_buffer::{DynamicState, AutoCommandBuffer, AutoCommandBufferBuilder};
use vulkano::descriptor::pipeline_layout::PipelineLayoutAbstract;
use vulkano::device::Device;
use vulkano::device::Queue;
use vulkano::instance::QueueFamily;
use vulkano::format::ClearValue;
use vulkano::format::*;
use vulkano::framebuffer::RenderPassDesc;
use vulkano::framebuffer::Framebuffer;
use vulkano::framebuffer::FramebufferAbstract;
use vulkano::framebuffer::RenderPassDescClearValues;
use vulkano::buffer::TypedBufferAccess;
use vulkano::buffer::BufferSlice;
use vulkano::buffer::BufferUsage;
use vulkano::buffer::CpuAccessibleBuffer;
use vulkano::pipeline::GraphicsPipelineAbstract;
use vulkano::pipeline::vertex::VertexSource;
use vulkano::descriptor::descriptor_set::collection::DescriptorSetsCollection;
use vulkano::descriptor::descriptor_set::PersistentDescriptorSet;
use vulkano::descriptor::descriptor_set::PersistentDescriptorSetBuf;
use vulkano::descriptor::descriptor_set::DescriptorSetDesc;
use vulkano::descriptor::descriptor::DescriptorDesc;
use vulkano::buffer::immutable::ImmutableBuffer;
use vulkano::descriptor::descriptor_set::DescriptorSet;
use vulkano::sampler::Sampler;
use vulkano::image::ImmutableImage;
use vulkano::image::Dimensions;
use vulkano::image::ImageUsage;
use vulkano::image::ImageLayout;
use vulkano::image::MipmapsCount;
use vulkano::image::traits::ImageAccess;
use vulkano::image::traits::ImageViewAccess;
use gltf::{self, Document, Gltf};
use gltf::mesh::util::ReadIndices;
use gltf::mesh::{Primitive, Mesh, Semantic};
use gltf::accessor::Accessor as GltfAccessor;
use gltf::Node;
use gltf::scene::Transform;
use gltf::Scene;
use gltf::accessor::DataType;
use gltf::image::Format as GltfFormat;
use failure::Error;
use ::Position;
use ::PipelineImpl;
use ::NodeUBO;
use ::MaterialUBO;
use ::MainDescriptorSet;
use math::matrix::Mat4;
use math::matrix::Matrix;
use self::error::*;

// #[derive(Clone)]
// struct MeasuredDescriptorSetsCollection {
//     collection: Arc<dyn DescriptorSetsCollection>,
//     sets: usize,
// }

// #[derive(Clone)]
// struct DescriptorSetVec {
//     collections: Vec<MeasuredDescriptorSetsCollection>,
// }

// impl DescriptorSetVec {
//     pub fn new(slice: &[Arc<dyn DescriptorSetsCollection>]) -> Self {
//         let mut collections = Vec::with_capacity(slice.len());

//         for collection in slice {
//             let mut sets = 0;

//             while let Some(_) = collection.num_bindings_in_set(sets) {
//                 sets += 1;
//             }

//             collections.push(MeasuredDescriptorSetsCollection {
//                 collection: collection.clone(),
//                 sets,
//             });
//         }

//         DescriptorSetVec {
//             collections
//         }
//     }

//     pub fn collection_by_set(&self, set: usize) -> Option<(&MeasuredDescriptorSetsCollection, usize)> {
//         unimplemented!()
//     }
// }

// unsafe impl DescriptorSetsCollection for DescriptorSetVec {
//     fn into_vec(self) -> Vec<Box<DescriptorSet + Send + Sync>> {
//         let len = self.collections.iter().map(|collection| collection.sets).sum();
//         let mut result = Vec::with_capacity(len);

//         for measured_collection in self.collections.into_iter() {
//             let collection = measured_collection.collection;
//             let mut subresult = collection.into_vec();
//             result.append(&mut subresult);
//         }

//         result
//     }

//     fn num_bindings_in_set(&self, set: usize) -> Option<usize> {
//         self.collection_by_set(set).and_then(|(collection, rel_index)| {
//             collection.collection.num_bindings_in_set(rel_index)
//         })
//     }

//     fn descriptor(&self, set: usize, binding: usize) -> Option<DescriptorDesc> {
//         self.collection_by_set(set).and_then(|(collection, rel_index)| {
//             collection.collection.descriptor(rel_index, binding)
//         })
//     }
// }

// #[derive(Clone)]
// struct DescriptorSetCollectionAppend<A, R>
//         where A: DescriptorSet + DescriptorSetDesc + Send + Sync + 'static,
//               R: DescriptorSetsCollection {
//     rest_len: usize,
//     rest: R,
//     append: A,
// }

// // unsafe impl<A, R> Send for DescriptorSetCollectionAppend<A, R>
// //         where A: DescriptorSet + DescriptorSetDesc + Send + Sync + 'static,
// //               R: DescriptorSetsCollection + Send {}
// // unsafe impl<A, R> Sync for DescriptorSetCollectionAppend<A, R>
// //         where A: DescriptorSet + DescriptorSetDesc + Send + Sync + 'static,
// //               R: DescriptorSetsCollection + Sync {}

// impl<A, R> DescriptorSetCollectionAppend<A, R>
//         where A: DescriptorSet + DescriptorSetDesc + Send + Sync + 'static,
//               R: DescriptorSetsCollection {
//     pub fn new(rest: R, append: A) -> Self {
//         let mut rest_len = 0;

//         while let Some(_) = rest.num_bindings_in_set(rest_len) {
//             rest_len += 1;
//         }

//         DescriptorSetCollectionAppend {
//             rest_len,
//             rest,
//             append,
//         }
//     }
// }

// unsafe impl<A, R> DescriptorSetsCollection for DescriptorSetCollectionAppend<A, R>
//     where A: DescriptorSet + DescriptorSetDesc + Send + Sync + 'static,
//           R: DescriptorSetsCollection {
//     fn into_vec(self) -> Vec<Box<DescriptorSet + Send + Sync>> {
//         let mut result: Vec<Box<DescriptorSet + Send + Sync>> = self.rest.into_vec();

//         result.push(Box::new(self.append));

//         result
//     }

//     fn num_bindings_in_set(&self, set: usize) -> Option<usize> {
//         if set == self.rest_len {
//             Some(self.append.num_bindings())
//         } else {
//             self.rest.num_bindings_in_set(set)
//         }
//     }

//     fn descriptor(&self, set: usize, binding: usize) -> Option<DescriptorDesc> {
//         if set == self.rest_len {
//             self.append.descriptor(binding)
//         } else {
//             self.rest.descriptor(set, binding)
//         }
//     }
// }

// TODO: Remove generics
#[derive(Clone)]
pub struct InitializationDrawContext<'a, F, C, RPD>
    where F: FramebufferAbstract + RenderPassDescClearValues<C> + Send + Sync + 'static,
          RPD: RenderPassDesc + RenderPassDescClearValues<Vec<ClearValue>> + Send + Sync + 'static {
    pub device: Arc<Device>,
    pub queue_family: QueueFamily<'a>,
    pub framebuffer: Arc<F>,
    pub clear_values: C,
    pub pipeline: PipelineImpl<RPD>,
    pub dynamic: &'a DynamicState,
    pub main_descriptor_set: MainDescriptorSet<RPD>,
}

#[derive(Clone)]
pub struct DrawContext<'a, RPD>
    where RPD: RenderPassDesc + RenderPassDescClearValues<Vec<ClearValue>> + Send + Sync + 'static {
    device: Arc<Device>,
    queue_family: QueueFamily<'a>,
    pipeline: PipelineImpl<RPD>,
    dynamic: &'a DynamicState,
    main_descriptor_set: MainDescriptorSet<RPD>,
}

pub enum InitializationTask {
    Buffer {
        index: usize,
        data: gltf::buffer::Data,
        initialization_buffer: Box<dyn TypedBufferAccess<Content=[u8]> + Send + Sync>,
    },
    Image {
        index: usize,
        data: gltf::image::Data,
        device_image: Box<dyn ImageAccess + Send + Sync>,
    },
    NodeDescriptorSet {
        index: usize,
        data: NodeUBO,
        initialization_buffer: Box<dyn TypedBufferAccess<Content=NodeUBO> + Send + Sync>,
    },
    MaterialDescriptorSet {
        index: usize,
        data: MaterialUBO,
        initialization_buffer: Box<dyn TypedBufferAccess<Content=MaterialUBO> + Send + Sync>,
    },
}

impl fmt::Debug for InitializationTask {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InitializationTask::Buffer { index, .. } => {
                write!(f, "buffer #{}", index)
            },
            InitializationTask::Image { index, .. } => {
                write!(f, "image #{}", index)
            },
            InitializationTask::NodeDescriptorSet { index, .. } => {
                write!(f, "node descriptor set #{}", index)
            },
            InitializationTask::MaterialDescriptorSet { index, .. } => {
                write!(f, "material descriptor set #{}", index)
            },
        }
    }
}

impl InitializationTask {
    fn initialize(self, device: Arc<Device>, command_buffer_builder: AutoCommandBufferBuilder) -> Result<AutoCommandBufferBuilder, Error> {
        match self {
            InitializationTask::Buffer { data, initialization_buffer, .. } => {
                let staging_buffer: Arc<CpuAccessibleBuffer<[u8]>> = CpuAccessibleBuffer::from_iter(
                    device,
                    BufferUsage::all(),
                    data.iter().cloned(), // FIXME: Single memcpy call, do not iterate
                )?;

                Ok(command_buffer_builder.copy_buffer(staging_buffer, initialization_buffer)?)
            },
            InitializationTask::Image { data, device_image, .. } => {
                let staging_buffer = CpuAccessibleBuffer::from_iter(
                    device,
                    BufferUsage::transfer_source(),
                    data.pixels.iter().cloned(), // FIXME: Single memcpy call, do not iterate
                )?;

                Ok(command_buffer_builder.copy_buffer_to_image(staging_buffer, device_image)?)
            },
            InitializationTask::NodeDescriptorSet { data, initialization_buffer, .. } => {
                let staging_buffer: Arc<CpuAccessibleBuffer<NodeUBO>> = CpuAccessibleBuffer::from_data(
                    device,
                    BufferUsage::all(),
                    data,
                )?;

                Ok(command_buffer_builder.copy_buffer(staging_buffer, initialization_buffer)?)
            },
            InitializationTask::MaterialDescriptorSet { data, initialization_buffer, .. } => {
                let staging_buffer: Arc<CpuAccessibleBuffer<MaterialUBO>> = CpuAccessibleBuffer::from_data(
                    device,
                    BufferUsage::all(),
                    data,
                )?;

                Ok(command_buffer_builder.copy_buffer(staging_buffer, initialization_buffer)?)
            },
        }
    }
}

pub struct Model {
    document: Document,
    device_buffers: Vec<Arc<ImmutableBuffer<[u8]>>>,
    device_images: Vec<Arc<dyn ImageViewAccess + Send + Sync>>,
    // Note: Do not ever try to express the descriptor set explicitly.
    node_descriptor_sets: Vec<Arc<dyn DescriptorSet + Send + Sync>>,
    material_descriptor_sets: Vec<Arc<dyn DescriptorSet + Send + Sync>>,
    initialization_tasks: Option<Vec<InitializationTask>>,
}

fn get_node_matrices_impl(parent: Option<&Node>, node: &Node, results: &mut Vec<Option<Mat4>>) {
    // Matrix and its children already calculated, bail.
    if let Some(_) = results[node.index()] {
        return;
    }

    results[node.index()] = Some(if let Some(parent) = parent {
        results[parent.index()].as_ref().unwrap() * Mat4(node.transform().matrix())
    } else {
        Mat4(node.transform().matrix())
    });

    for child in node.children() {
        get_node_matrices_impl(Some(node), &child, results);
    }
}

fn get_node_matrices(document: &Document) -> Vec<Mat4> {
    let mut results = Vec::with_capacity(document.nodes().len());

    for _ in 0..document.nodes().len() {
        results.push(None);
    }

    for scene in document.scenes() {
        for node in scene.nodes() {
            get_node_matrices_impl(None, &node, &mut results);
        }
    }

    results.into_iter().map(|option| option.unwrap_or_else(Mat4::identity)).collect()
}

impl Model {
    pub fn import<'a, I, S>(device: Arc<Device>, queue: Arc<Queue>, queue_families: I, pipeline: PipelineImpl<impl RenderPassDesc + Send + Sync + 'static>, path: S)
            -> Result<Model, Error>
            where I: IntoIterator<Item = QueueFamily<'a>> + Clone,
                  S: AsRef<Path> {
        let (document, buffer_data_array, image_data_array) = gltf::import(path)?;
        let mut initialization_tasks: Vec<InitializationTask> = Vec::with_capacity(
            buffer_data_array.len() + image_data_array.len() + document.nodes().len()
        );
        let mut device_buffers: Vec<Arc<ImmutableBuffer<[u8]>>> = Vec::with_capacity(buffer_data_array.len());

        for (index, buffer_data) in buffer_data_array.into_iter().enumerate() {
            let (device_buffer, buffer_initialization) = unsafe {
                ImmutableBuffer::raw(
                    device.clone(),
                    buffer_data.len(),
                    BufferUsage { // TODO: Scan document for buffer usage and optimize
                        transfer_destination: true,
                        uniform_buffer: true,
                        storage_buffer: true,
                        index_buffer: true,
                        vertex_buffer: true,
                        indirect_buffer: true,
                        ..BufferUsage::none()
                    },
                    queue_families.clone(),
                )
            }?;
            initialization_tasks.push(InitializationTask::Buffer {
                index,
                data: buffer_data,
                initialization_buffer: Box::new(buffer_initialization),
            });
            device_buffers.push(device_buffer);
        }

        let mut device_images: Vec<Arc<dyn ImageViewAccess + Send + Sync>> = Vec::with_capacity(document.textures().len());

        for (index, image_data) in image_data_array.into_iter().enumerate() {
            macro_rules! push_image_with_format {
                ($vk_format:expr) => {{
                    let (device_image, image_initialization) = ImmutableImage::uninitialized(
                        device.clone(),
                        Dimensions::Dim2d {
                            width: image_data.width,
                            height: image_data.height,
                        },
                        $vk_format,
                        MipmapsCount::Log2,
                        ImageUsage {
                            transfer_destination: true,
                            sampled: true,
                            ..ImageUsage::none()
                        },
                        ImageLayout::ShaderReadOnlyOptimal,
                        queue_families.clone(),
                    )?;
                    initialization_tasks.push(InitializationTask::Image {
                        index,
                        data: image_data,
                        device_image: Box::new(image_initialization),
                    });
                    device_images.push(device_image);
                }}
            }

            match image_data.format {
                GltfFormat::R8 => push_image_with_format!(R8Uint),
                GltfFormat::R8G8 => push_image_with_format!(R8G8Uint),
                GltfFormat::R8G8B8 => push_image_with_format!(R8G8B8A8Srgb),
                GltfFormat::R8G8B8A8 => push_image_with_format!(R8G8B8A8Srgb),
            }
        }

        let mut node_descriptor_sets: Vec<Arc<dyn DescriptorSet + Send + Sync>> = Vec::with_capacity(document.nodes().len());
        let transform_matrices = get_node_matrices(&document);

        for node in document.nodes() {
            let node_ubo = NodeUBO::new(transform_matrices[node.index()].clone());
            let (device_buffer, buffer_initialization) = unsafe {
                ImmutableBuffer::<NodeUBO>::uninitialized(
                    device.clone(),
                    BufferUsage::uniform_buffer_transfer_destination(),
                )
            }?;
            let descriptor_set = Arc::new(
                PersistentDescriptorSet::start(pipeline.clone(), 1)
                    .add_buffer(device_buffer.clone()).unwrap()
                    .build().unwrap()
            );

            initialization_tasks.push(InitializationTask::NodeDescriptorSet {
                index: node.index(),
                data: node_ubo,
                initialization_buffer: Box::new(buffer_initialization),
            });
            node_descriptor_sets.push(descriptor_set);
        }

        let mut material_descriptor_sets: Vec<Arc<dyn DescriptorSet + Send + Sync>> = Vec::with_capacity(document.materials().len());

        for material in document.materials() {
            let pbr = material.pbr_metallic_roughness();
            let material_ubo = MaterialUBO {
                base_color_factor: pbr.base_color_factor(),
                metallic_factor: pbr.metallic_factor(),
                roughness_factor: pbr.roughness_factor(),
            };
            let (device_buffer, buffer_initialization) = unsafe {
                ImmutableBuffer::<MaterialUBO>::uninitialized(
                    device.clone(),
                    BufferUsage::uniform_buffer_transfer_destination(),
                )
            }?;
            let base_color_texture_index = pbr.base_color_texture().map(|it| it.texture().index()).unwrap_or(0);
            let base_color_texture = device_images[base_color_texture_index].clone();
            let descriptor_set = Arc::new(
                PersistentDescriptorSet::start(pipeline.clone(), 2)
                    .add_buffer(device_buffer.clone()).unwrap()
                    .add_sampled_image(
                        base_color_texture,
                        Sampler::simple_repeat_linear(device.clone())
                    ).unwrap()
                    .build().unwrap()
            );

            initialization_tasks.push(InitializationTask::MaterialDescriptorSet {
                index: material.index().expect("Implicit material definitions are not supported yet."), // FIXME
                data: material_ubo,
                initialization_buffer: Box::new(buffer_initialization),
            });
            material_descriptor_sets.push(descriptor_set);
        }

        Ok(Model {
            document,
            device_buffers,
            node_descriptor_sets,
            material_descriptor_sets,
            device_images,
            initialization_tasks: Some(initialization_tasks),
        })
    }

    pub fn initialize(&mut self, device: Arc<Device>, mut command_buffer_builder: AutoCommandBufferBuilder) -> Result<AutoCommandBufferBuilder, Error> {
        if let Some(initialization_tasks) = self.initialization_tasks.take() {
            for initialization_task in initialization_tasks.into_iter() {
                command_buffer_builder = initialization_task.initialize(device.clone(), command_buffer_builder)?;
            }

            Ok(command_buffer_builder)
        } else {
            Err(ModelInitializationError::ModelAlreadyInitialized.into())
        }
    }

    pub fn draw_scene<F, C, RPD>(&self, context: InitializationDrawContext<F, C, RPD>, scene_index: usize) -> Result<AutoCommandBuffer, Error>
            where F: FramebufferAbstract + RenderPassDescClearValues<C> + Send + Sync + 'static,
                  RPD: RenderPassDesc + RenderPassDescClearValues<Vec<ClearValue>> + Send + Sync + 'static {
        if scene_index >= self.document.scenes().len() {
            return Err(ModelDrawError::InvalidSceneIndex { index: scene_index }.into());
        }

        let InitializationDrawContext {
            device,
            queue_family,
            framebuffer,
            clear_values,
            pipeline,
            dynamic,
            main_descriptor_set,
        } = context;

        let scene = self.document.scenes().nth(scene_index).unwrap();
        let mut command_buffer = AutoCommandBufferBuilder::primary_one_time_submit(device.clone(), queue_family.clone())
            .unwrap()
            .begin_render_pass(framebuffer.clone(), false, clear_values).unwrap();
        let draw_context = DrawContext {
            device: device,
            queue_family: queue_family,
            pipeline: pipeline,
            dynamic: dynamic,
            main_descriptor_set: main_descriptor_set,
        };

        for node in scene.nodes() {
            command_buffer = self.draw_node(node, command_buffer, &draw_context);
        }

        command_buffer = command_buffer.end_render_pass().unwrap();

        Ok(command_buffer.build().unwrap())
    }

    pub fn draw_main_scene<F, C, RPD>(&self, context: InitializationDrawContext<F, C, RPD>) -> Result<AutoCommandBuffer, Error>
            where F: FramebufferAbstract + RenderPassDescClearValues<C> + Send + Sync + 'static,
                  RPD: RenderPassDesc + RenderPassDescClearValues<Vec<ClearValue>> + Send + Sync + 'static {
        if let Some(main_scene_index) = self.document.default_scene().map(|default_scene| default_scene.index()) {
            self.draw_scene(context, main_scene_index)
        } else {
            Err(ModelDrawError::NoDefaultScene.into())
        }
    }

    pub fn draw_node<'a, RPD>(&self, node: Node<'a>, mut command_buffer: AutoCommandBufferBuilder, context: &DrawContext<RPD>)
        -> AutoCommandBufferBuilder
        where RPD: RenderPassDesc + RenderPassDescClearValues<Vec<ClearValue>> + Send + Sync + 'static {

        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                let material = primitive.material();
                let material_index = material.index().expect("Implicit material definitions are not supported yet."); // FIXME

                let descriptor_sets = (
                    context.main_descriptor_set.clone(),
                    self.node_descriptor_sets[node.index()].clone(),
                    self.material_descriptor_sets[material_index].clone(),
                );

                command_buffer = self.draw_primitive(&primitive, command_buffer, context, descriptor_sets.clone());
            }
        }

        for child in node.children() {
            command_buffer = self.draw_node(child, command_buffer, context);
        }

        command_buffer
    }

    pub fn draw_primitive<'a, S, RPD>(&self, primitive: &Primitive<'a>, mut command_buffer: AutoCommandBufferBuilder, context: &DrawContext<RPD>, sets: S)
        -> AutoCommandBufferBuilder
        where S: DescriptorSetsCollection + Clone,
              RPD: RenderPassDesc + RenderPassDescClearValues<Vec<ClearValue>> + Send + Sync + 'static {
        let positions_accessor = primitive.get(&Semantic::Positions).unwrap();
        let indices_accessor = primitive.indices();

        let vertex_slice: BufferSlice<[Position], Arc<ImmutableBuffer<[u8]>>> = {
            let buffer_view = positions_accessor.view();
            let buffer_index = buffer_view.buffer().index();
            let buffer_offset = positions_accessor.offset() + buffer_view.offset();
            let buffer_bytes = positions_accessor.size() * positions_accessor.count();

            let vertex_buffer = self.device_buffers[buffer_index].clone();
            let vertex_slice = BufferSlice::from_typed_buffer_access(vertex_buffer)
                .slice(buffer_offset..(buffer_offset + buffer_bytes))
                .unwrap();

            unsafe { vertex_slice.reinterpret::<[Position]>() }
        };

        if let Some(indices_accessor) = indices_accessor {
            macro_rules! draw_indexed {
                ($index_type:ty; $command_buffer:ident, $context:ident, $vertex_slice:ident, $indices_accessor:ident, $sets:ident) => {
                    let index_slice: BufferSlice<[$index_type], Arc<ImmutableBuffer<[u8]>>> = {
                        let buffer_view = $indices_accessor.view();
                        let buffer_index = buffer_view.buffer().index();
                        let buffer_offset = $indices_accessor.offset() + buffer_view.offset();
                        let buffer_bytes = $indices_accessor.size() * $indices_accessor.count();

                        let index_buffer = self.device_buffers[buffer_index].clone();
                        let index_slice = BufferSlice::from_typed_buffer_access(index_buffer)
                            .slice(buffer_offset..(buffer_offset + buffer_bytes))
                            .unwrap();

                        unsafe { index_slice.reinterpret::<[$index_type]>() }
                    };

                    // unsafe {
                    //     let index_slice: BufferSlicePublic<[u16], Arc<CpuAccessibleBuffer<[u8]>>> = mem::transmute(index_slice);
                    //     println!("index_slice: {:?}", index_slice);
                    // }

                    $command_buffer = $command_buffer.draw_indexed(
                        $context.pipeline.clone(),
                        $context.dynamic,
                        $vertex_slice,
                        index_slice,
                        $sets.clone(),
                        () /* push_constants */).unwrap();
                }
            }

            match indices_accessor.data_type() {
                DataType::U16 => {
                    draw_indexed!(u16; command_buffer, context, vertex_slice, indices_accessor, sets);
                },
                DataType::U32 => {
                    draw_indexed!(u32; command_buffer, context, vertex_slice, indices_accessor, sets);
                },
                _ => {
                    panic!("Index type not supported.");
                }
            }
        } else {
            command_buffer = command_buffer.draw(
                context.pipeline.clone(),
                context.dynamic,
                vertex_slice,
                sets.clone(),
                () /* push_constants */).unwrap();
        }

        command_buffer
    }
}

// #[derive(Debug)]
// pub struct BufferSlicePublic<T: ?Sized, B> {
//     pub marker: PhantomData<T>,
//     pub resource: B,
//     pub offset: usize,
//     pub size: usize,
// }
