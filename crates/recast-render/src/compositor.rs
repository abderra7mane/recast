use std::{
    collections::HashMap,
    num::NonZeroU64,
    sync::{Arc, mpsc},
};

use bytemuck::{Pod, Zeroable};
use recast_project::BackgroundFill;
use wgpu::util::DeviceExt;

use crate::{
    CpuFrame, Error, FrameSource, PixelFormat, Result,
    annotations::{CellGrid, Raster, pack},
    bitmap::Rgba,
    layout::{Layout, layout},
    scene::Scene,
};

const MAX_RIPPLES: usize = 16;
/// Blurred and pixelated areas drawn in one frame; `obscure.wgsl` has the same limit.
const MAX_AREAS: usize = 64;
/// Width of the atlas holding the cells of blurred and pixelated areas.
const ATLAS_WIDTH: u32 = 2_048;
const ATLAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Annotation images drawn in one frame.
const MAX_ANNOTATIONS: usize = 256;
/// Annotation images unused for this many frames leave the GPU.
const RASTER_IDLE_FRAMES: u64 = 600;
/// Bytes of annotation images kept on the GPU that the last frame didn't draw.
const SPARE_GPU_BYTES: usize = 256 << 20;
/// How much a pressed cursor shrinks.
const SQUISH: f64 = 0.18;
/// Ripple ring thickness in screen points.
const RIPPLE_RING_PTS: f64 = 2.5;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CompositeParams {
    canvas_content: [f32; 4],
    content_size: [f32; 4],
    shadow: [f32; 4],
    view: [f32; 4],
    bg_a: [f32; 4],
    bg_b: [f32; 4],
    gradient: [f32; 4],
    ripple_color: [f32; 4],
    source: [f32; 4],
    ripples: [[f32; 4]; MAX_RIPPLES],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CursorParams {
    rect: [f32; 4],
    canvas: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct AnnotationParams {
    rect: [f32; 4],
    canvas: [f32; 4],
    content: [f32; 4],
    view: [f32; 4],
    source: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Area {
    rect: [f32; 4],
    grid: [f32; 4],
    info: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Areas {
    count: [f32; 4],
    items: [Area; MAX_AREAS],
}

/// Cell averages of blurred and pixelated areas, and their smoothed copy.
struct Atlas {
    height: u32,
    cells: wgpu::TextureView,
    smooth: wgpu::TextureView,
    smooth_bind: wgpu::BindGroup,
}

struct PlacedArea {
    grid: CellGrid,
    corner: (u32, u32),
    /// Its place among the annotations shown, in drawing order.
    order: usize,
}

struct GpuRaster {
    raster: Arc<Raster>,
    bind: wgpu::BindGroup,
    used: u64,
    bytes: usize,
}

struct ScreenTexture {
    texture: wgpu::Texture,
    width: u32,
    height: u32,
    format: PixelFormat,
    frame_id: Option<u64>,
    has_alpha: bool,
}

struct SceneTextures {
    scene_id: u64,
    background: wgpu::Texture,
    cursors: HashMap<Option<u32>, wgpu::BindGroup>,
}

/// Draws frames of a scene off screen with wgpu and reads them back.
pub struct Compositor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    target: wgpu::Texture,
    readback: wgpu::Buffer,
    padded_row: u32,
    composite_pipeline: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
    composite_params: wgpu::Buffer,
    composite_bind: Option<wgpu::BindGroup>,
    cursor_pipeline: wgpu::RenderPipeline,
    cursor_layout: wgpu::BindGroupLayout,
    cursor_params: wgpu::Buffer,
    sampler: wgpu::Sampler,
    screen: Option<ScreenTexture>,
    scene: Option<SceneTextures>,
    areas: wgpu::Buffer,
    obscure_layout: wgpu::BindGroupLayout,
    cells_pipeline: wgpu::RenderPipeline,
    smooth_pipeline: wgpu::RenderPipeline,
    atlas: Atlas,
    annotation_pipeline: wgpu::RenderPipeline,
    annotation_layout: wgpu::BindGroupLayout,
    annotation_params: wgpu::Buffer,
    annotation_stride: u64,
    rasters: HashMap<String, GpuRaster>,
    /// The image each annotation was last drawn with, by annotation id: a changed
    /// annotation keeps it until its new image arrives.
    shown: HashMap<String, (String, Arc<Raster>)>,
    frames: u64,
    missing: usize,
}

fn texture_format(format: PixelFormat) -> wgpu::TextureFormat {
    match format {
        PixelFormat::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
        PixelFormat::Bgra8 => wgpu::TextureFormat::Bgra8Unorm,
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32, size: usize) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(size as u64),
        },
        count: None,
    }
}

/// A uniform bound at a different offset for each draw.
fn dynamic_uniform_entry(binding: u32, size: usize) -> wgpu::BindGroupLayoutEntry {
    let mut entry = uniform_entry(binding, size);
    if let wgpu::BindingType::Buffer {
        has_dynamic_offset, ..
    } = &mut entry.ty
    {
        *has_dynamic_offset = true;
    }
    entry
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

struct PipelineSpec<'a> {
    label: &'a str,
    source: &'a str,
    fragment: &'a str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    topology: wgpu::PrimitiveTopology,
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    PipelineSpec {
        label,
        source,
        fragment,
        format,
        blend,
        topology,
    }: PipelineSpec<'_>,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// The frame texture and its readback buffer, with the buffer's row stride.
fn size_resources(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> Result<(wgpu::Texture, wgpu::Buffer, u32)> {
    let max = device.limits().max_texture_dimension_2d;
    if width == 0 || height == 0 || width > max || height > max {
        return Err(Error::Gpu(format!(
            "frame size {width}×{height} is outside 1…{max}"
        )));
    }
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let padded_row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: padded_row as u64 * height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    Ok((target, readback, padded_row))
}

fn create_atlas(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    areas: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    height: u32,
) -> Atlas {
    let texture = |label| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: ATLAS_WIDTH,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: ATLAS_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    let cells = texture("cells");
    let smooth = texture("smoothed cells");
    let smooth_bind = obscure_bind(device, layout, areas, sampler, &cells);
    Atlas {
        height,
        cells,
        smooth,
        smooth_bind,
    }
}

fn obscure_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    areas: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    source: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("obscure"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: areas.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(source),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

impl Compositor {
    /// A compositor drawing `width × height` frames in `format`, without a window.
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| Error::Gpu(format!("no GPU adapter: {e}")))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("recast"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| Error::Gpu(format!("cannot open the GPU: {e}")))?;
        let target_format = texture_format(format);
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite"),
            entries: &[
                uniform_entry(0, size_of::<CompositeParams>()),
                texture_entry(1),
                texture_entry(2),
                sampler_entry(3),
                uniform_entry(4, size_of::<Areas>()),
                texture_entry(5),
                texture_entry(6),
            ],
        });
        let cursor_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cursor"),
            entries: &[
                uniform_entry(0, size_of::<CursorParams>()),
                texture_entry(1),
                sampler_entry(2),
            ],
        });
        let annotation_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("annotation"),
            entries: &[
                dynamic_uniform_entry(0, size_of::<AnnotationParams>()),
                texture_entry(1),
                sampler_entry(2),
                uniform_entry(3, size_of::<Areas>()),
            ],
        });
        let obscure_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("obscure"),
            entries: &[
                uniform_entry(0, size_of::<Areas>()),
                texture_entry(1),
                sampler_entry(2),
            ],
        });
        let composite_pipeline = pipeline(
            &device,
            &composite_layout,
            PipelineSpec {
                label: "composite",
                source: include_str!("shaders/composite.wgsl"),
                fragment: "fs",
                format: target_format,
                blend: None,
                topology: wgpu::PrimitiveTopology::TriangleList,
            },
        );
        let image_spec = |label| PipelineSpec {
            label,
            source: include_str!("shaders/cursor.wgsl"),
            fragment: "fs",
            format: target_format,
            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            topology: wgpu::PrimitiveTopology::TriangleStrip,
        };
        let cursor_pipeline = pipeline(&device, &cursor_layout, image_spec("cursor"));
        let annotation_pipeline = pipeline(
            &device,
            &annotation_layout,
            PipelineSpec {
                source: include_str!("shaders/annotation.wgsl"),
                ..image_spec("annotation")
            },
        );
        let obscure_spec = |label, fragment| PipelineSpec {
            label,
            source: include_str!("shaders/obscure.wgsl"),
            fragment,
            format: ATLAS_FORMAT,
            blend: None,
            topology: wgpu::PrimitiveTopology::TriangleList,
        };
        let cells_pipeline = pipeline(&device, &obscure_layout, obscure_spec("cells", "fs_cells"));
        let smooth_pipeline = pipeline(
            &device,
            &obscure_layout,
            obscure_spec("smooth", "fs_smooth"),
        );
        let uniform = |label, size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let composite_params = uniform("composite params", size_of::<CompositeParams>() as u64);
        let cursor_params = uniform("cursor params", size_of::<CursorParams>() as u64);
        let areas = uniform("areas", size_of::<Areas>() as u64);
        let annotation_stride = (size_of::<AnnotationParams>() as u64)
            .next_multiple_of(device.limits().min_uniform_buffer_offset_alignment as u64);
        let annotation_params = uniform(
            "annotation params",
            annotation_stride * MAX_ANNOTATIONS as u64,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let atlas = create_atlas(&device, &obscure_layout, &areas, &sampler, 64);

        let (target, readback, padded_row) = size_resources(&device, target_format, width, height)?;
        Ok(Self {
            device,
            queue,
            format: target_format,
            width,
            height,
            target,
            readback,
            padded_row,
            composite_pipeline,
            composite_layout,
            composite_params,
            composite_bind: None,
            cursor_pipeline,
            cursor_layout,
            cursor_params,
            sampler,
            screen: None,
            scene: None,
            areas,
            obscure_layout,
            cells_pipeline,
            smooth_pipeline,
            atlas,
            annotation_pipeline,
            annotation_layout,
            annotation_params,
            annotation_stride,
            rasters: HashMap::new(),
            shown: HashMap::new(),
            frames: 0,
            missing: 0,
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Changes the frame size, keeping the GPU device and pipelines.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        if (width, height) == (self.width, self.height) {
            return Ok(());
        }
        let (target, readback, padded_row) =
            size_resources(&self.device, self.format, width, height)?;
        self.target = target;
        self.readback = readback;
        self.padded_row = padded_row;
        self.width = width;
        self.height = height;
        self.scene = None;
        self.composite_bind = None;
        Ok(())
    }

    /// The rendered frame, for drawing it elsewhere on the GPU.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.target
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    fn upload_mips(&self, label: &str, image: &Rgba) -> wgpu::Texture {
        let levels = image.mip_chain();
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, mip) in levels.iter().enumerate() {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &mip.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(mip.width * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: mip.width,
                    height: mip.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        texture
    }

    /// Background image scaled on the CPU to cover the frame exactly.
    fn background_texture(&self, scene: &Scene) -> wgpu::Texture {
        let cover = scene
            .background
            .as_ref()
            .filter(|_| matches!(scene.settings.background.fill, BackgroundFill::Image { .. }))
            .map(|image| cover(image, self.width, self.height))
            .unwrap_or_else(|| Rgba {
                width: 1,
                height: 1,
                pixels: vec![0, 0, 0, 255],
            });
        self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some("background"),
                size: wgpu::Extent3d {
                    width: cover.width,
                    height: cover.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &cover.pixels,
        )
    }

    fn prepare_scene(&mut self, scene: &Scene) {
        if self
            .scene
            .as_ref()
            .is_some_and(|s| s.scene_id == scene.id())
        {
            return;
        }
        let background = self.background_texture(scene);
        let mut cursors = HashMap::new();
        for (id, cursor) in scene.cursor_images() {
            let texture = self.upload_mips("cursor", &cursor.image);
            let view = texture.create_view(&Default::default());
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cursor"),
                layout: &self.cursor_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.cursor_params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            cursors.insert(id, bind);
        }
        self.scene = Some(SceneTextures {
            scene_id: scene.id(),
            background,
            cursors,
        });
        self.composite_bind = None;
    }

    fn upload_frame(&mut self, frame: &CpuFrame<'_>) -> Result<()> {
        let row = frame.width as usize * 4;
        let needed = frame.bytes_per_row * (frame.height as usize).saturating_sub(1) + row;
        if frame.width == 0
            || frame.height == 0
            || frame.bytes_per_row < row
            || frame.data.len() < needed
        {
            return Err(Error::Source(format!(
                "frame of {}×{} with {} bytes per row does not fit {} bytes",
                frame.width,
                frame.height,
                frame.bytes_per_row,
                frame.data.len()
            )));
        }
        let reusable = self.screen.as_ref().is_some_and(|s| {
            s.width == frame.width && s.height == frame.height && s.format == frame.format
        });
        if !reusable {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("screen"),
                size: wgpu::Extent3d {
                    width: frame.width,
                    height: frame.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: texture_format(frame.format),
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.screen = Some(ScreenTexture {
                texture,
                width: frame.width,
                height: frame.height,
                format: frame.format,
                frame_id: None,
                has_alpha: frame.has_alpha,
            });
            self.composite_bind = None;
        }
        let screen = self.screen.as_mut().expect("screen texture");
        screen.has_alpha = frame.has_alpha;
        if frame.id.is_some() && screen.frame_id == frame.id {
            return Ok(());
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &screen.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.data[..needed],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.bytes_per_row as u32),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
        screen.frame_id = frame.id;
        Ok(())
    }

    fn composite_bind(&mut self) -> &wgpu::BindGroup {
        if self.composite_bind.is_none() {
            let screen = self
                .screen
                .as_ref()
                .expect("frame uploaded")
                .texture
                .create_view(&Default::default());
            let background = self
                .scene
                .as_ref()
                .expect("scene prepared")
                .background
                .create_view(&Default::default());
            self.composite_bind = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("composite"),
                layout: &self.composite_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.composite_params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&screen),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&background),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.areas.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&self.atlas.cells),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&self.atlas.smooth),
                    },
                ],
            }));
        }
        self.composite_bind.as_ref().expect("bind group")
    }

    /// Lays out the cells of the blurred and pixelated areas shown at `t_ms` in the
    /// atlas, growing it when needed, and writes their parameters.
    fn prepare_areas(&mut self, scene: &Scene, t_ms: f64) -> Result<Vec<PlacedArea>> {
        let screen = self.screen.as_ref().expect("frame uploaded");
        let texture = (screen.width, screen.height);
        let (mut order, mut grids): (Vec<usize>, Vec<CellGrid>) = scene
            .annotations_at(t_ms)
            .enumerate()
            .filter_map(|(i, (a, _))| {
                CellGrid::new(&a.shape, (scene.screen_width, scene.screen_height), texture)
                    .map(|grid| (i, grid))
            })
            .unzip();
        if grids.len() > MAX_AREAS {
            log::warn!(
                "{} blurred or pixelated areas at {t_ms} ms; only {MAX_AREAS} are drawn",
                grids.len()
            );
            order.drain(..grids.len() - MAX_AREAS);
            grids.drain(..grids.len() - MAX_AREAS);
        }
        let sizes: Vec<_> = grids.iter().map(|g| (g.columns, g.rows)).collect();
        let (corners, height) = pack(&sizes, ATLAS_WIDTH);
        if height > self.atlas.height {
            let height = height.next_power_of_two();
            if height > self.device.limits().max_texture_dimension_2d {
                return Err(Error::Gpu(
                    "the blurred and pixelated areas don't fit on the GPU".into(),
                ));
            }
            self.atlas = create_atlas(
                &self.device,
                &self.obscure_layout,
                &self.areas,
                &self.sampler,
                height,
            );
            self.composite_bind = None;
        }
        let mut areas = Areas::zeroed();
        areas.count[0] = grids.len() as f32;
        for (area, (grid, corner)) in areas.items.iter_mut().zip(grids.iter().zip(&corners)) {
            *area = Area {
                rect: [grid.x, grid.y, grid.width, grid.height].map(|v| v as f32),
                grid: [corner.0, corner.1, grid.columns, grid.rows].map(|v| v as f32),
                info: [
                    grid.block as f32,
                    if grid.pixelate { 1.0 } else { 0.0 },
                    0.0,
                    0.0,
                ],
            };
        }
        self.queue
            .write_buffer(&self.areas, 0, bytemuck::bytes_of(&areas));
        Ok(grids
            .into_iter()
            .zip(corners)
            .zip(order)
            .map(|((grid, corner), order)| PlacedArea {
                grid,
                corner,
                order,
            })
            .collect())
    }

    /// Averages the cells of `grids` and smooths the blurred ones.
    fn encode_areas(&self, encoder: &mut wgpu::CommandEncoder, grids: &[PlacedArea]) {
        if grids.is_empty() {
            return;
        }
        let screen = self
            .screen
            .as_ref()
            .expect("frame uploaded")
            .texture
            .create_view(&Default::default());
        let cells_bind = obscure_bind(
            &self.device,
            &self.obscure_layout,
            &self.areas,
            &self.sampler,
            &screen,
        );
        let passes = [
            (&self.atlas.cells, &self.cells_pipeline, &cells_bind, true),
            (
                &self.atlas.smooth,
                &self.smooth_pipeline,
                &self.atlas.smooth_bind,
                false,
            ),
        ];
        for (target, pipeline, bind, all) in passes {
            if !all && grids.iter().all(|a| a.grid.pixelate) {
                continue;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cells"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind, &[]);
            for (i, area) in grids.iter().enumerate() {
                let (grid, (x, y)) = (&area.grid, area.corner);
                if all || !grid.pixelate {
                    pass.set_scissor_rect(x, y, grid.columns, grid.rows);
                    pass.draw(0..3, i as u32..i as u32 + 1);
                }
            }
        }
    }

    /// The bind group of an annotation image, uploading it when it is new.
    fn raster_bind(&mut self, key: &str, raster: Arc<Raster>) -> wgpu::BindGroup {
        let frames = self.frames;
        if let Some(gpu) = self.rasters.get_mut(key)
            && Arc::ptr_eq(&gpu.raster, &raster)
        {
            gpu.used = frames;
            return gpu.bind.clone();
        }
        let image = Rgba::from_straight(raster.width, raster.height, raster.rgba.clone());
        let view = self
            .upload_mips("annotation", &image)
            .create_view(&Default::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("annotation"),
            layout: &self.annotation_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.annotation_params,
                        offset: 0,
                        size: NonZeroU64::new(size_of::<AnnotationParams>() as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.areas.as_entire_binding(),
                },
            ],
        });
        // The image and its smaller copies.
        let bytes = raster.rgba.len() * 4 / 3;
        self.rasters.insert(
            key.to_string(),
            GpuRaster {
                raster,
                bind: bind.clone(),
                used: frames,
                bytes,
            },
        );
        bind
    }

    /// Frees annotation images the last frame didn't draw: the ones unused for long, and
    /// the least recently used beyond [`SPARE_GPU_BYTES`].
    fn evict_rasters(&mut self) {
        let frames = self.frames;
        let mut spare: Vec<(u64, usize, String)> = self
            .rasters
            .iter()
            .filter(|(_, gpu)| gpu.used != frames)
            .map(|(key, gpu)| (gpu.used, gpu.bytes, key.clone()))
            .collect();
        spare.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        let mut kept = 0;
        for (used, bytes, key) in spare {
            kept += bytes;
            if kept > SPARE_GPU_BYTES || used + RASTER_IDLE_FRAMES < frames {
                self.rasters.remove(&key);
            }
        }
    }

    /// How many annotations the last frame couldn't draw with their current image.
    pub fn missing_rasters(&self) -> usize {
        self.missing
    }

    pub fn layout(&self, scene: &Scene) -> Layout {
        layout(
            self.width,
            self.height,
            scene.video_width,
            scene.video_height,
            &scene.settings.background,
        )
    }

    /// Renders the frame at `t_ms` (recording time), taking the video frame from `source`.
    pub fn render_from(
        &mut self,
        scene: &Scene,
        source: &mut dyn FrameSource,
        t_ms: f64,
    ) -> Result<()> {
        let frame = source.frame_at(t_ms)?;
        self.render(scene, &frame, t_ms)
    }

    /// Renders the frame at `t_ms` (recording time) over `frame`, the video frame shown then.
    pub fn render(&mut self, scene: &Scene, frame: &CpuFrame<'_>, t_ms: f64) -> Result<()> {
        self.prepare_scene(scene);
        self.upload_frame(frame)?;
        let areas = self.prepare_areas(scene, t_ms)?;
        let layout = self.layout(scene);
        let settings = &scene.settings;
        let camera = scene.timeline.camera(t_ms);
        let (vx, vy, vw, vh) = camera.view();
        let content = layout.content;
        let px_per_pt = content.width / scene.screen_width.max(1e-9);
        let zoomed_px_per_pt = px_per_pt * camera.scale;
        let to_pixels = |p: recast_zoom::Point| {
            let v = camera.project(p);
            (
                content.x + v.x * content.width,
                content.y + v.y * content.height,
            )
        };

        let mut ripples = [[0f32; 4]; MAX_RIPPLES];
        let mut ripple_count = 0;
        if settings.clicks.ripple && settings.clicks.size > 0.0 {
            let active: Vec<_> = scene.timeline.ripples(t_ms).collect();
            for ripple in active.iter().rev().take(MAX_RIPPLES) {
                let (x, y) = to_pixels(ripple.pos);
                let grow = 1.0 - (1.0 - ripple.progress).powi(3);
                let radius = settings.clicks.size * zoomed_px_per_pt * grow;
                let alpha = (1.0 - ripple.progress).powi(2);
                ripples[ripple_count] = [x as f32, y as f32, radius as f32, alpha as f32];
                ripple_count += 1;
            }
        }

        let (bg_kind, bg_a, bg_b, direction) = match &settings.background.fill {
            BackgroundFill::Solid { color } => (0.0, color.to_f32(), color.to_f32(), [0.0, 0.0]),
            BackgroundFill::Gradient {
                from,
                to,
                angle_deg,
            } => {
                let angle = angle_deg.to_radians();
                (
                    1.0,
                    from.to_f32(),
                    to.to_f32(),
                    [angle.sin() as f32, -angle.cos() as f32],
                )
            }
            BackgroundFill::Image { .. } => (2.0, [0.0; 4], [0.0; 4], [0.0, 0.0]),
        };
        let screen = self.screen.as_ref().expect("screen texture");
        let params = CompositeParams {
            canvas_content: [
                self.width as f32,
                self.height as f32,
                content.x as f32,
                content.y as f32,
            ],
            content_size: [
                content.width as f32,
                content.height as f32,
                layout.corner_radius as f32,
                layout.shadow_blur as f32,
            ],
            shadow: [
                layout.shadow_offset_y as f32,
                settings.background.shadow.opacity as f32,
                bg_kind,
                ripple_count as f32,
            ],
            view: [vx as f32, vy as f32, vw as f32, vh as f32],
            bg_a,
            bg_b,
            gradient: [direction[0], direction[1], 0.0, 0.0],
            ripple_color: settings.clicks.color.to_f32(),
            source: [
                screen.width as f32,
                screen.height as f32,
                (RIPPLE_RING_PTS * zoomed_px_per_pt).max(1.5) as f32,
                if screen.has_alpha { 1.0 } else { 0.0 },
            ],
            ripples,
        };
        self.queue
            .write_buffer(&self.composite_params, 0, bytemuck::bytes_of(&params));

        let cursor = scene.timeline.cursor(t_ms).filter(|c| c.opacity > 0.0);
        let cursor_draw = cursor.map(|state| {
            let (image, id) = scene.cursor(state.shape);
            let squish = if settings.clicks.squish {
                1.0 - SQUISH * state.press
            } else {
                1.0
            };
            let scale = zoomed_px_per_pt * settings.cursor.size * squish;
            let (x, y) = to_pixels(state.pos);
            let params = CursorParams {
                rect: [
                    (x - image.hotspot_x * scale) as f32,
                    (y - image.hotspot_y * scale) as f32,
                    (image.width_pts * scale) as f32,
                    (image.height_pts * scale) as f32,
                ],
                canvas: [
                    self.width as f32,
                    self.height as f32,
                    state.opacity as f32,
                    0.0,
                ],
            };
            self.queue
                .write_buffer(&self.cursor_params, 0, bytemuck::bytes_of(&params));
            id
        });

        self.frames += 1;
        self.missing = 0;
        let screen = self.screen.as_ref().expect("screen texture");
        let source = [screen.width as f32, screen.height as f32, 0.0, 0.0];
        let mut annotations = Vec::new();
        let mut annotation_params = Vec::new();
        for (order, (annotation, key)) in scene.annotations_at(t_ms).enumerate() {
            let Some(key) = key else { continue };
            let (key, raster) = match scene.rasters.get(key) {
                Some(raster) => {
                    self.shown
                        .insert(annotation.id.clone(), (key.to_string(), raster.clone()));
                    (key.to_string(), raster)
                }
                None => {
                    self.missing += 1;
                    match self.shown.get(&annotation.id) {
                        Some((key, raster)) => (key.clone(), raster.clone()),
                        None => continue,
                    }
                }
            };
            if annotations.len() == MAX_ANNOTATIONS {
                log::warn!("more than {MAX_ANNOTATIONS} annotations at {t_ms} ms");
                break;
            }
            let anchor = annotation.shape.anchor();
            let (x, y) = to_pixels(recast_zoom::Point::new(
                (anchor.x + raster.x) / scene.screen_width.max(1e-9),
                (anchor.y + raster.y) / scene.screen_height.max(1e-9),
            ));
            let first_area = areas.partition_point(|a| a.order < order);
            let params = AnnotationParams {
                rect: [
                    x as f32,
                    y as f32,
                    (raster.width_pts * zoomed_px_per_pt) as f32,
                    (raster.height_pts * zoomed_px_per_pt) as f32,
                ],
                canvas: [
                    self.width as f32,
                    self.height as f32,
                    1.0,
                    first_area as f32,
                ],
                content: [content.x, content.y, content.width, content.height].map(|v| v as f32),
                view: [vx, vy, vw, vh].map(|v| v as f32),
                source,
            };
            let offset = annotation_params.len();
            annotation_params.extend_from_slice(bytemuck::bytes_of(&params));
            annotation_params.resize(offset + self.annotation_stride as usize, 0);
            annotations.push((self.raster_bind(&key, raster), offset as u32));
        }
        if !annotation_params.is_empty() {
            self.queue
                .write_buffer(&self.annotation_params, 0, &annotation_params);
        }
        // An annotation the editor draws itself while it changes must not come back with
        // its old image when it lands.
        if let Some(hidden) = &scene.hidden {
            self.shown.remove(hidden);
        }
        self.shown
            .retain(|id, _| scene.settings.annotations.items.iter().any(|a| &a.id == id));
        self.evict_rasters();

        let target = self.target.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        self.encode_areas(&mut encoder, &areas);
        {
            let composite_bind = self.composite_bind().clone();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &composite_bind, &[]);
            pass.draw(0..3, 0..1);

            let clip = clip_rect(&layout, self.width, self.height);
            if let Some(id) = cursor_draw
                && let Some((x, y, w, h)) = clip
            {
                let bind = &self.scene.as_ref().expect("scene prepared").cursors[&id];
                pass.set_scissor_rect(x, y, w, h);
                pass.set_pipeline(&self.cursor_pipeline);
                pass.set_bind_group(0, bind, &[]);
                pass.draw(0..4, 0..1);
            }
            if !annotations.is_empty()
                && let Some((x, y, w, h)) = clip
            {
                pass.set_scissor_rect(x, y, w, h);
                pass.set_pipeline(&self.annotation_pipeline);
                for (bind, offset) in &annotations {
                    pass.set_bind_group(0, bind, &[*offset]);
                    pass.draw(0..4, 0..1);
                }
            }
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        Ok(())
    }

    /// Gives `read` the last rendered frame: its rows are `bytes_per_row` apart and
    /// each starts with `width × 4` bytes of pixels in the compositor's format.
    pub fn read<R>(&self, read: impl FnOnce(&[u8], usize) -> R) -> Result<R> {
        let slice = self.readback.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| Error::Gpu(e.to_string()))?;
        rx.recv()
            .map_err(|e| Error::Gpu(e.to_string()))?
            .map_err(|e| Error::Gpu(e.to_string()))?;
        let result = {
            let data = slice
                .get_mapped_range()
                .map_err(|e| Error::Gpu(e.to_string()))?;
            read(&data, self.padded_row as usize)
        };
        self.readback.unmap();
        Ok(result)
    }

    /// The last rendered frame as tightly packed pixels.
    pub fn read_packed(&self) -> Result<Vec<u8>> {
        let row = self.width as usize * 4;
        self.read(|data, stride| {
            let mut out = Vec::with_capacity(row * self.height as usize);
            for y in 0..self.height as usize {
                out.extend_from_slice(&data[y * stride..y * stride + row]);
            }
            out
        })
    }

    /// Renders and reads back one frame as tightly packed pixels.
    pub fn frame(
        &mut self,
        scene: &Scene,
        source: &mut dyn FrameSource,
        t_ms: f64,
    ) -> Result<Vec<u8>> {
        self.render_from(scene, source, t_ms)?;
        self.read_packed()
    }
}

/// The screen area in whole pixels; the cursor is clipped to it.
fn clip_rect(layout: &Layout, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let c = layout.content;
    let x0 = c.x.round().clamp(0.0, width as f64) as u32;
    let y0 = c.y.round().clamp(0.0, height as f64) as u32;
    let x1 = (c.x + c.width).round().clamp(0.0, width as f64) as u32;
    let y1 = (c.y + c.height).round().clamp(0.0, height as f64) as u32;
    (x1 > x0 && y1 > y0).then(|| (x0, y0, x1 - x0, y1 - y0))
}

/// Scales and crops `image` to fill `width × height`, keeping its aspect ratio.
fn cover(image: &Rgba, width: u32, height: u32) -> Rgba {
    let scale = (width as f64 / image.width as f64).max(height as f64 / image.height as f64);
    let scaled_w = ((image.width as f64 * scale).ceil() as u32).max(width);
    let scaled_h = ((image.height as f64 * scale).ceil() as u32).max(height);
    let source = image::RgbaImage::from_raw(image.width, image.height, image.pixels.clone())
        .expect("pixel buffer matches size");
    let scaled = image::imageops::resize(
        &source,
        scaled_w,
        scaled_h,
        image::imageops::FilterType::Triangle,
    );
    let x = (scaled_w - width) / 2;
    let y = (scaled_h - height) / 2;
    let cropped = image::imageops::crop_imm(&scaled, x, y, width, height).to_image();
    Rgba {
        width,
        height,
        pixels: cropped.into_raw(),
    }
}
