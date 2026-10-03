//! Draws triangles with the PlayStation's texturing: the texture page and
//! CLUT of each polygon select texels and colours out of a copy of VRAM, the
//! colour 0x0000 is transparent, and the texel is modulated by the vertex
//! colour with 128 as 1.0.

use wgpu::util::DeviceExt;

use crate::mesh::{VRAM_H, VRAM_W, Vram, Vtx};

const SHADER: &str = r#"
struct Camera { mvp: mat4x4f }
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var vram: texture_2d<u32>;

struct In {
    @location(0) pos: vec3f,
    @location(1) colour: u32,
    @location(2) uv: u32,
    @location(3) mode: u32,
}

struct Out {
    @builtin(position) pos: vec4f,
    @location(0) colour: vec3f,
    @location(1) uv: vec2f,
    @location(2) @interpolate(flat) mode: u32,
}

@vertex
fn vs(v: In) -> Out {
    var o: Out;
    o.pos = camera.mvp * vec4f(v.pos, 1.0);
    o.colour = vec3f(f32(v.colour & 255u), f32((v.colour >> 8u) & 255u), f32((v.colour >> 16u) & 255u));
    o.uv = vec2f(f32(v.uv & 255u), f32((v.uv >> 8u) & 255u));
    o.mode = v.mode;
    return o;
}

fn word(x: u32, y: u32) -> u32 {
    return textureLoad(vram, vec2u(x & 1023u, y & 511u), 0).r;
}

@fragment
fn fs(i: Out) -> @location(0) vec4f {
    let tpage = i.mode >> 16u;
    let clut = i.mode & 0xffffu;
    let bx = (tpage & 15u) * 64u;
    let by = ((tpage >> 4u) & 1u) * 256u;
    let depth = (tpage >> 7u) & 3u;
    let cx = (clut & 63u) * 16u;
    let cy = clut >> 6u;
    let u = u32(clamp(i.uv.x, 0.0, 255.0));
    let v = u32(clamp(i.uv.y, 0.0, 255.0));
    var c: u32;
    if depth == 0u {
        let idx = (word(bx + u / 4u, by + v) >> ((u % 4u) * 4u)) & 15u;
        c = word(cx + idx, cy);
    } else if depth == 1u {
        let idx = (word(bx + u / 2u, by + v) >> ((u % 2u) * 8u)) & 255u;
        c = word(cx + idx, cy);
    } else {
        c = word(bx + u, by + v);
    }
    if c == 0u {
        discard;
    }
    let t = vec3f(f32(c & 31u), f32((c >> 5u) & 31u), f32((c >> 10u) & 31u)) * 8.0;
    let rgb = clamp(t * i.colour / 128.0, vec3f(0.0), vec3f(255.0)) / 255.0;
    return vec4f(rgb, 1.0);
}
"#;

pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    camera: wgpu::Buffer,
    /// What does not move (the track), uploaded once.
    vertices: wgpu::Buffer,
    count: u32,
    /// What moves (the cars), replaced each frame by `set_moving`.
    moving: Option<(wgpu::Buffer, u32)>,
    depth: Option<(wgpu::Texture, u32, u32)>,
    pub clear: [u8; 3],
}

pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        vram: &Vram,
        tris: &[Vtx],
    ) -> Renderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let vram_tex = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("vram"),
                size: wgpu::Extent3d { width: VRAM_W as u32, height: VRAM_H as u32, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R16Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &vram.words.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>(),
        );
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let view = vram_tex.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: camera.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Uint32, 2 => Uint32, 3 => Uint32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("world"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vtx>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
            },
            // The game draws a face only when it is front-facing; the
            // triangles are wound counter-clockwise for front.
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("triangles"),
            contents: Vtx::bytes(tris),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Renderer {
            pipeline,
            group,
            camera,
            vertices,
            count: tris.len() as u32,
            moving: None,
            depth: None,
            clear: [0, 0, 0],
        }
    }

    /// The triangles that move, drawn after the fixed ones until replaced.
    pub fn set_moving(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, tris: &[Vtx]) {
        let bytes = Vtx::bytes(tris);
        let fits = self.moving.as_ref().is_some_and(|(b, _)| b.size() >= bytes.len() as u64);
        if !fits {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("moving"),
                size: (bytes.len() as u64).max(64).next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.moving = Some((buffer, 0));
        }
        if let Some((buffer, count)) = &mut self.moving {
            queue.write_buffer(buffer, 0, bytes);
            *count = tris.len() as u32;
        }
    }

    /// Draws into `target` with the camera's view-projection matrix.
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: (u32, u32),
        mvp: glam::Mat4,
    ) -> wgpu::CommandBuffer {
        if self.depth.as_ref().is_none_or(|d| (d.1, d.2) != size) {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth = Some((t, size.0, size.1));
        }
        let depth_view = self.depth.as_ref().unwrap().0.create_view(&Default::default());
        let bytes: Vec<u8> = mvp.to_cols_array().iter().flat_map(|f| f.to_le_bytes()).collect();
        queue.write_buffer(&self.camera, 0, &bytes);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let c = self.clear.map(|v| v as f64 / 255.0);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: c[0], g: c[1], b: c[2], a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.draw(0..self.count, 0..1);
            if let Some((buffer, count)) = &self.moving
                && *count > 0
            {
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*count, 0..1);
            }
        }
        encoder.finish()
    }
}
