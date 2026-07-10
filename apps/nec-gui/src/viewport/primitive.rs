// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! GUI-CHK-002 — the `shader::Primitive` that renders the wire scene (wires +
//! axes + ground grid) as a colored line-list into iced's own wgpu frame.
//!
//! It shares iced's `Device`/`Queue`/`target` (no swapchain of its own), draws
//! with `LoadOp::Load` so iced's UI beneath is preserved, and owns a private
//! `Depth32Float` texture (iced provides none) recreated on resize. GPU
//! resources persist in the shader `Storage`; the vertex buffer is re-uploaded
//! only when the scene revision changes.

use std::sync::Arc;

use iced::widget::shader::{self, wgpu};
use iced::Rectangle;
use nec_gui::mesh::{LineVertex, MeshData};

/// A cheap per-frame handle bundle: the camera matrix plus an `Arc` to the
/// current mesh and its revision (buffers re-upload only when `rev` changes).
#[derive(Debug)]
pub struct ScenePrimitive {
    pub view_proj: [[f32; 4]; 4],
    pub mesh: Option<Arc<MeshData>>,
    pub rev: u64,
}

/// GPU resources cached in the shader `Storage` (keyed by type) across frames.
struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertices: Option<wgpu::Buffer>,
    vertex_count: u32,
    uploaded_rev: Option<u64>,
    depth: wgpu::TextureView,
    depth_size: (u32, u32),
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

impl Pipeline {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, size: (u32, u32)) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gui.viewport.lines"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/lines.wgsl").into()),
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gui.viewport.camera"),
            size: 64, // one mat4x4<f32>
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gui.viewport.bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gui.viewport.bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gui.viewport.layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gui.viewport.lines.pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 12,
                            shader_location: 1,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        Self {
            pipeline,
            uniform,
            bind_group,
            vertices: None,
            vertex_count: 0,
            uploaded_rev: None,
            depth: create_depth(device, size),
            depth_size: size,
        }
    }

    fn ensure_depth(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.depth_size != size && size.0 > 0 && size.1 > 0 {
            self.depth = create_depth(device, size);
            self.depth_size = size;
        }
    }

    fn upload_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &MeshData,
        rev: u64,
    ) {
        if self.uploaded_rev == Some(rev) {
            return;
        }
        let bytes: &[u8] = bytemuck::cast_slice(&mesh.vertices);
        let need = bytes.len() as u64;
        let have = self.vertices.as_ref().map_or(0, wgpu::Buffer::size);
        if have < need || self.vertices.is_none() {
            self.vertices = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gui.viewport.vertices"),
                size: need.max(16),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if let Some(buf) = &self.vertices {
            queue.write_buffer(buf, 0, bytes);
        }
        self.vertex_count = mesh.vertices.len() as u32;
        self.uploaded_rev = Some(rev);
    }
}

fn create_depth(device: &wgpu::Device, (w, h): (u32, u32)) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gui.viewport.depth"),
        size: wgpu::Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

impl shader::Primitive for ScenePrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        _bounds: &Rectangle,
        viewport: &shader::Viewport,
    ) {
        let size = viewport.physical_size();
        let size = (size.width, size.height);
        if !storage.has::<Pipeline>() {
            storage.store(Pipeline::new(device, format, size));
        }
        let Some(p) = storage.get_mut::<Pipeline>() else {
            return;
        };
        p.ensure_depth(device, size);
        queue.write_buffer(&p.uniform, 0, bytemuck::cast_slice(&[self.view_proj]));
        if let Some(mesh) = &self.mesh {
            p.upload_mesh(device, queue, mesh, self.rev);
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(p) = storage.get::<Pipeline>() else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gui.viewport.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &p.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        pass.set_viewport(
            clip_bounds.x as f32,
            clip_bounds.y as f32,
            clip_bounds.width as f32,
            clip_bounds.height as f32,
            0.0,
            1.0,
        );
        let (Some(vbuf), true) = (&p.vertices, p.vertex_count > 0) else {
            return;
        };
        pass.set_pipeline(&p.pipeline);
        pass.set_bind_group(0, &p.bind_group, &[]);
        pass.set_vertex_buffer(0, vbuf.slice(..));
        pass.draw(0..p.vertex_count, 0..1);
    }
}
