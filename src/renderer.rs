//! WebGPU renderer: same world and camera math as the engines before it,
//! with triangle rasterization (depth test, texturing) on the GPU.
//! Platform-agnostic: frontends create the `wgpu::Instance` + `Surface`
//! (canvas on wasm, window on native) and pass them in.

use crate::math::Mat4;
use crate::obj::{Mesh, Texture, Tri};
use std::collections::HashMap;

const STRIDE: u64 = 32; // pos3 + uv2 + color3
const SKY: [f32; 3] = [232.0 / 255.0, 200.0 / 255.0, 156.0 / 255.0];

const SHADER: &str = r#"
struct Uniforms {
  viewProj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

struct VSIn {
  @location(0) pos: vec3<f32>,
  @location(1) uv: vec2<f32>,
  @location(2) color: vec3<f32>,
};
struct VSOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) color: vec3<f32>,
};

@vertex fn vs(in: VSIn) -> VSOut {
  var out: VSOut;
  out.clip = u.viewProj * vec4<f32>(in.pos, 1.0);
  out.uv = in.uv;
  out.color = in.color;
  return out;
}

@fragment fn fs(in: VSOut) -> @location(0) vec4<f32> {
  return vec4<f32>(textureSample(tex, smp, in.uv).rgb * in.color, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vtx {
    pos: [f32; 3],
    uv: [f32; 2],
    color: [f32; 3],
}

struct Group {
    buffer: wgpu::Buffer,
    count: u32,
    bind_group: wgpu::BindGroup,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    depth: wgpu::Texture,
    off: wgpu::Texture,
    readbuf: wgpu::Buffer,
    groups: Vec<Group>,
    format: wgpu::TextureFormat,
    w: u32,
    h: u32,
    view_proj: Mat4,
}

impl Renderer {
    pub async fn new(
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        present_mode: wgpu::PresentMode,
        w: u32,
        h: u32,
        textures: &[Texture],
        meshes: &[Mesh],
    ) -> Result<Renderer, String> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| e.to_string())?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| e.to_string())?;
        // Prefer a plain (non-sRGB) format so readback bytes match the wasm
        // canvas exactly; Metal lists Bgra8UnormSrgb first by default.
        let formats = surface.get_capabilities(&adapter).formats;
        let format = if formats.contains(&wgpu::TextureFormat::Bgra8Unorm) {
            wgpu::TextureFormat::Bgra8Unorm
        } else if formats.contains(&wgpu::TextureFormat::Rgba8Unorm) {
            wgpu::TextureFormat::Rgba8Unorm
        } else {
            formats[0]
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: w,
            height: h,
            present_mode,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 4,
        };
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: STRIDE,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 12, shader_location: 1 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 20, shader_location: 2 },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
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
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth = create_tex(&device, w, h, wgpu::TextureFormat::Depth24Plus, wgpu::TextureUsages::RENDER_ATTACHMENT);
        // Offscreen target for readback (canvas textures are not COPY_SRC).
        let off = create_tex(&device, w, h, format, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
        let readbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (w * h * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
 
         // GPU copies of all textures; flat-color triangles share a 1x1 white
         // texture with the shade carried by the vertex color.
         let mut gpu_textures: Vec<wgpu::Texture> = textures.iter().map(|t| upload_texture(&device, &queue, t)).collect();
        gpu_textures.push(make_white(&device, &queue));
        let white_idx = (gpu_textures.len() - 1) as u32;

        let mut by_tex: HashMap<u32, Vec<&Tri>> = HashMap::new();
        for mesh in meshes {
            for tri in &mesh.tris {
                let key = if tri.tex >= 0 { tri.tex as u32 } else { white_idx };
                by_tex.entry(key).or_default().push(tri);
            }
        }
        let mut groups = Vec::new();
        for (tex_idx, tris) in by_tex {
            let flat = tex_idx == white_idx;
            let mut verts: Vec<Vtx> = Vec::with_capacity(tris.len() * 3);
            for tri in &tris {
                for v in [&tri.a, &tri.b, &tri.c] {
                    let (cr, cg, cb) = if flat {
                        let c = tri.color;
                        let s = tri.shade;
                        (
                            ((c >> 16) & 255) as f32 / 255.0 * s,
                            ((c >> 8) & 255) as f32 / 255.0 * s,
                            (c & 255) as f32 / 255.0 * s,
                        )
                    } else {
                        (tri.shade, tri.shade, tri.shade)
                    };
                    verts.push(Vtx { pos: [v.x, v.y, v.z], uv: [v.u, v.v], color: [cr, cg, cb] });
                }
            }
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: (verts.len() * std::mem::size_of::<Vtx>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&verts));
            let view = gpu_textures[tex_idx as usize].create_view(&Default::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bind_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
                ],
            });
            groups.push(Group { buffer, count: (tris.len() * 3) as u32, bind_group });
        }

        Ok(Renderer {
            device,
            queue,
            surface,
            pipeline,
            uniform,
            depth,
            off,
            readbuf,
            groups,
            format,
            w,
            h,
            view_proj: Mat4([0.0; 16]),
        })
    }

    /// Draw one frame to the surface with the current camera matrix.
    pub fn render(&mut self, view_proj: &Mat4) {
        self.view_proj = *view_proj;
        self.queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&[view_proj.0]));
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            other => {
                eprintln!("surface acquire: {other:?}");
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        self.draw(&mut enc, &view);
        self.queue.submit([enc.finish()]);
    }

    /// Render the current camera into the surface and copy the presented
    /// texture back as RGBA (diagnostic: verifies the surface path).
    pub async fn capture_surface(&mut self) -> Result<Vec<u8>, String> {
        self.queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&[self.view_proj.0]));
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            other => return Err(format!("surface: {other:?}")),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        self.draw(&mut enc, &view);
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &frame.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readbuf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.w * 4), rows_per_image: None },
            },
            wgpu::Extent3d { width: self.w, height: self.h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        self.map_readbuf().await
    }

    /// Render the current camera into the offscreen target and read it back
    /// as tightly packed RGBA bytes (used for numeric verification).
    pub async fn read_frame(&mut self) -> Result<Vec<u8>, String> {
        self.queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&[self.view_proj.0]));
        let off_view = self.off.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        self.draw(&mut enc, &off_view);
        self.queue.submit([enc.finish()]);

        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &self.off, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readbuf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.w * 4), rows_per_image: None },
            },
            wgpu::Extent3d { width: self.w, height: self.h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        self.map_readbuf().await
    }

    async fn map_readbuf(&mut self) -> Result<Vec<u8>, String> {
        let (tx, rx) = futures_channel::oneshot::channel();
        let data = {
            let slice = self.readbuf.slice(..);
            slice.map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            match rx.await {
                Ok(Ok(())) => {}
                _ => return Err(String::from("buffer map failed")),
            }
            let data = slice
                .get_mapped_range()
                .map_err(|e| format!("map range: {e:?}"))?
                .to_vec();
            data
        };
        self.readbuf.unmap();
        let mut data = data;
        if matches!(self.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
            for px in data.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        Ok(data)
    }

    /// Record the scene pass (sky clear + all draw groups) into `enc`.
    fn draw(&self, enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: SKY[0] as f64, g: SKY[1] as f64, b: SKY[2] as f64, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth.create_view(&Default::default()),
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        for g in &self.groups {
            pass.set_bind_group(0, &g.bind_group, &[]);
            pass.set_vertex_buffer(0, g.buffer.slice(..));
            pass.draw(0..g.count, 0..1);
        }
    }

    /// Surface dimensions in physical pixels.
    pub fn dims(&self) -> (u32, u32) {
        (self.w, self.h)
    }
}

fn create_tex(device: &wgpu::Device, w: u32, h: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

// Rows must be 256-byte aligned for writeTexture; pad when needed.
fn upload_texture(device: &wgpu::Device, queue: &wgpu::Queue, tex: &Texture) -> wgpu::Texture {
    let gpu = create_tex(device, tex.w, tex.h, wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
    let row_bytes = tex.w * 4;
    let padded = row_bytes.div_ceil(256) * 256;
    if padded == row_bytes {
        write_tex(queue, &gpu, &tex.data, padded, tex.h);
    } else {
        // Padded rows start zeroed; copy only each row's real bytes (JS
        // subarray().set() clamps the same way).
        let mut buf = vec![0u8; (padded * tex.h) as usize];
        for y in 0..tex.h as usize {
            let (start, dstart) = (y * padded as usize, y * row_bytes as usize);
            buf[start..start + row_bytes as usize]
                .copy_from_slice(&tex.data[dstart..dstart + row_bytes as usize]);
        }
        write_tex(queue, &gpu, &buf, padded, tex.h);
    }
    gpu
}

fn write_tex(queue: &wgpu::Queue, gpu: &wgpu::Texture, data: &[u8], bytes_per_row: u32, h: u32) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: gpu, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bytes_per_row), rows_per_image: Some(h) },
        wgpu::Extent3d { width: gpu.width(), height: h, depth_or_array_layers: 1 },
    );
}

fn make_white(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let gpu = create_tex(device, 1, 1, wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &gpu, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &[255, 255, 255, 255],
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: None },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    gpu
}
