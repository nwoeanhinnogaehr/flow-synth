use gui::geom::*;

use std::sync::{Arc, Mutex};

use gfx;
use gfx::texture;
use gfx::traits::{Factory, FactoryExt};
use gfx::{Encoder, IntoIndexBuffer, PipelineState, Slice};
use gfx::memory::{Bind, Usage};
use gfx::buffer::Role;
use gfx::handle::{Buffer, DepthStencilView, RenderTargetView, Sampler, ShaderResourceView};
use gfx_text;
use gfx_device_gl as gl;

pub type ColorFormat = gfx::format::Rgba8;
pub type DepthFormat = gfx::format::DepthStencil;

gfx_defines! {
    vertex Vertex {
        pos: [f32; 2] = "a_Pos",
    }

    vertex ColoredRect {
        translate: [f32; 3] = "a_Translate",
        scale: [f32; 2] = "a_Scale",
        color: [f32; 3] = "a_Color",
    }

    pipeline rect_pipe {
        resolution: gfx::Global<[f32; 2]> = "i_Resolution",
        vertices: gfx::VertexBuffer<Vertex> = (),
        instances: gfx::InstanceBuffer<ColoredRect> = (),
        out: gfx::BlendTarget<ColorFormat> = ("Target0", gfx::state::ColorMask::all(), gfx::preset::blend::ALPHA),
        depth: gfx::DepthTarget<DepthFormat> = gfx::state::Depth {
            fun: gfx::state::Comparison::LessEqual,
            write: true,
        },
    }

    vertex TexturedVertex {
        translate: [f32; 3] = "a_Translate",
        tex_coord: [f32; 2] = "a_TexCoord",
    }

    pipeline textured_rect_pipe {
        resolution: gfx::Global<[f32; 2]> = "i_Resolution",
        texture: gfx::TextureSampler<[f32; 4]> = "i_Texture",
        vertices: gfx::VertexBuffer<TexturedVertex> = (),
        out: gfx::BlendTarget<ColorFormat> = ("Target0", gfx::state::ColorMask::all(), gfx::preset::blend::ALPHA),
        depth: gfx::DepthTarget<DepthFormat> = gfx::state::Depth {
            fun: gfx::state::Comparison::LessEqual,
            write: true,
        },
    }

    vertex PipeVertex {
        translate: [f32; 3] = "a_Translate",
        color: [f32; 4] = "a_Color",
    }

    pipeline pipe_pipe { // pipe line
        resolution: gfx::Global<[f32; 2]> = "i_Resolution",
        vertices: gfx::VertexBuffer<PipeVertex> = (),
        out: gfx::BlendTarget<ColorFormat> = ("Target0", gfx::state::ColorMask::all(), gfx::preset::blend::ALPHA),
        depth: gfx::DepthTarget<DepthFormat> = gfx::state::Depth {
            fun: gfx::state::Comparison::LessEqual,
            write: true,
        },
    }
}
pub struct Target {
    pub color: RenderTargetView<gl::Resources, ColorFormat>,
    pub depth: DepthStencilView<gl::Resources, DepthFormat>,
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> [Vertex; 4] {
    [
        Vertex {
            pos: [x, y],
        },
        Vertex {
            pos: [x, y + h],
        },
        Vertex {
            pos: [x + w, y + h],
        },
        Vertex {
            pos: [x + w, y],
        },
    ]
}

const RECT_IDX: [u16; 6] = [0, 1, 2, 0, 2, 3];

#[derive(Clone)]
struct RectRenderer {
    factory: gl::Factory,
    pso: PipelineState<gl::Resources, rect_pipe::Meta>,
    slice: Slice<gl::Resources>,
    vertex_buffer: Buffer<gl::Resources, Vertex>,
    rects: Vec<ColoredRect>,
}
impl RectRenderer {
    fn new(mut factory: gl::Factory) -> RectRenderer {
        let pso = factory
            .create_pipeline_simple(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/shaders/rect_150.glslv"
                )),
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/shaders/rect_150.glslf"
                )),
                rect_pipe::new(),
            )
            .unwrap();
        let vertex_buffer = factory
            .create_buffer_immutable(&rect(0.0, 0.0, 1.0, 1.0), Role::Vertex, Bind::empty())
            .unwrap();
        let index_buffer = RECT_IDX[..].into_index_buffer(&mut factory);
        let buffer_length = match index_buffer {
            gfx::IndexBuffer::Auto => vertex_buffer.len(),
            gfx::IndexBuffer::Index16(ref ib) => ib.len(),
            gfx::IndexBuffer::Index32(ref ib) => ib.len(),
        };
        let slice = Slice {
            start: 0,
            end: buffer_length as u32,
            base_vertex: 0,
            instances: Some((0, 0)),
            buffer: index_buffer,
        };
        RectRenderer {
            factory,
            pso,
            slice,
            vertex_buffer,
            rects: Vec::new(),
        }
    }
    fn push(&mut self, rect: ColoredRect) {
        self.rects.push(rect);
    }
    fn draw(&mut self, encoder: &mut Encoder<gl::Resources, gl::CommandBuffer>, target: &Target) {
        let instance_buffer = self.factory
            .create_buffer_immutable(&self.rects, Role::Vertex, Bind::empty())
            .unwrap();
        let data = rect_pipe::Data {
            resolution: target_dimensions(target),
            vertices: self.vertex_buffer.clone(),
            instances: instance_buffer,
            out: target.color.clone(),
            depth: target.depth.clone(),
        };
        self.slice.instances = Some((self.rects.len() as u32, 0));
        encoder.draw(&self.slice, &self.pso, &data);
        self.rects.clear();
    }
}

#[derive(Clone)]
struct TexturedRectRenderer {
    factory: gl::Factory,
    pso: PipelineState<gl::Resources, textured_rect_pipe::Meta>,
    sampler: Sampler<gl::Resources>,
    rects: Vec<(Rect3, ShaderResourceView<gl::Resources, [f32; 4]>)>,
}

impl TexturedRectRenderer {
    fn new(mut factory: gl::Factory) -> TexturedRectRenderer {
        let pso = factory
            .create_pipeline_simple(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/shaders/textured_rect_150.glslv"
                )),
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/shaders/textured_rect_150.glslf"
                )),
                textured_rect_pipe::new(),
            )
            .unwrap();
        let sampler = factory.create_sampler_linear();
        TexturedRectRenderer {
            factory,
            pso,
            sampler,
            rects: Vec::new(),
        }
    }
    fn push(&mut self, rect: Rect3, texture: ShaderResourceView<gl::Resources, [f32; 4]>) {
        self.rects.push((rect, texture));
    }
    fn draw(&mut self, encoder: &mut Encoder<gl::Resources, gl::CommandBuffer>, target: &Target) {
        for (rect, texture) in self.rects.drain(..) {
            let vertices = [
                TexturedVertex {
                    translate: rect.pos.into(),
                    tex_coord: [0.0, 1.0],
                },
                TexturedVertex {
                    translate: (rect.pos + Pt3::new(0.0, rect.size.y, 0.0)).into(),
                    tex_coord: [0.0, 0.0],
                },
                TexturedVertex {
                    translate: (rect.pos + Pt3::new(rect.size.x, rect.size.y, 0.0)).into(),
                    tex_coord: [1.0, 0.0],
                },
                TexturedVertex {
                    translate: (rect.pos + Pt3::new(rect.size.x, 0.0, 0.0)).into(),
                    tex_coord: [1.0, 1.0],
                },
            ];
            let (vertex_buffer, slice) = self.factory
                .create_vertex_buffer_with_slice(&vertices, &RECT_IDX[..]);
            let data = textured_rect_pipe::Data {
                resolution: target_dimensions(target),
                texture: (texture, self.sampler.clone()),
                vertices: vertex_buffer,
                out: target.color.clone(),
                depth: target.depth.clone(),
            };
            encoder.draw(&slice, &self.pso, &data);
        }
    }
}

#[derive(Clone)]
struct TextRenderer {
    renderer: Arc<Mutex<gfx_text::Renderer<gl::Resources, gl::Factory>>>,
    texts: Vec<(String, [f32; 2], [f32; 3])>,
}
impl TextRenderer {
    fn new(factory: gl::Factory) -> TextRenderer {
        let renderer = Arc::new(Mutex::new(gfx_text::new(factory.clone()).unwrap()));
        TextRenderer {
            renderer,
            texts: Vec::new(),
        }
    }
    fn push(&mut self, text: &str, pos: [f32; 2], color: [f32; 3]) {
        self.texts.push((text.into(), pos, color));
    }
    fn draw(&mut self, encoder: &mut Encoder<gl::Resources, gl::CommandBuffer>, target: &Target) {
        let mut renderer = self.renderer.lock().unwrap();
        for (text, pos, color) in self.texts.drain(..) {
            renderer.add(
                &text,
                [pos[0] as i32, pos[1] as i32],
                [color[0], color[1], color[2], 1.0],
            );
        }
        renderer.draw(encoder, &target.color).unwrap();
    }
}

#[derive(Clone)]
struct PipeRenderer {
    paths: Vec<Vec<Pt3>>,
    pso: PipelineState<gl::Resources, pipe_pipe::Meta>,
    factory: gl::Factory,
}
impl PipeRenderer {
    fn new(mut factory: gl::Factory) -> PipeRenderer {
        let pso = factory
            .create_pipeline_simple(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/shaders/pipe_150.glslv"
                )),
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/shaders/pipe_150.glslf"
                )),
                pipe_pipe::new(),
            )
            .unwrap();
        PipeRenderer {
            paths: Vec::new(),
            pso,
            factory,
        }
    }
    fn push(&mut self, points: &[Pt3]) {
        self.paths.push(points.into());
    }
    fn draw(&mut self, encoder: &mut Encoder<gl::Resources, gl::CommandBuffer>, target: &Target) {
        const RADIUS: f32 = 2.0;
        let mut vertices: Vec<PipeVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        for path in self.paths.drain(..) {
            for window in path.windows(2) {
                let p0 = window[0];
                let p1 = window[1];
                let line = (p0 - p1).drop_z();
                let line = line / (line.x * line.x + line.y * line.y).sqrt();
                let line = Pt3::new(line.y, -line.x, 0.0);
                let idx0 = vertices.len() as u32;
                vertices.extend(&[
                    PipeVertex {
                        translate: (p0 - line * RADIUS).into(),
                        color: [0.0, 1.0, 0.0, 0.3],
                    },
                    PipeVertex {
                        translate: (p0 + line * RADIUS).into(),
                        color: [0.0, 1.0, 0.0, 0.3],
                    },
                    PipeVertex {
                        translate: (p1 - line * RADIUS).into(),
                        color: [0.0, 1.0, 0.0, 0.3],
                    },
                    PipeVertex {
                        translate: (p1 + line * RADIUS).into(),
                        color: [0.0, 1.0, 0.0, 0.3],
                    },
                ]);
                indices.extend(&[idx0, idx0 + 1, idx0 + 2, idx0 + 1, idx0 + 2, idx0 + 3]);
            }
        }
        let (vertex_buffer, slice) = self.factory
            .create_vertex_buffer_with_slice(&vertices, &indices[..]);
        let data = pipe_pipe::Data {
            resolution: target_dimensions(target),
            vertices: vertex_buffer,
            out: target.color.clone(),
            depth: target.depth.clone(),
        };
        encoder.draw(&slice, &self.pso, &data);
    }
}

pub struct RenderContext {
    factory: gl::Factory,
    encoder: Encoder<gl::Resources, gl::CommandBuffer>,
    rects: RectRenderer,
    textured_rects: TexturedRectRenderer,
    texts: TextRenderer,
    pipes: PipeRenderer,
}
impl RenderContext {
    pub fn new(mut factory: gl::Factory) -> RenderContext {
        let encoder = factory.create_command_buffer().into();
        let rects = RectRenderer::new(factory.clone());
        let textured_rects = TexturedRectRenderer::new(factory.clone());
        let texts = TextRenderer::new(factory.clone());
        let pipes = PipeRenderer::new(factory.clone());

        RenderContext {
            factory,
            encoder,
            rects,
            textured_rects,
            texts,
            pipes,
        }
    }
    pub fn begin_frame(&mut self, target: &Target) {
        self.encoder.clear(&target.color, [0.0; 4]);
        self.encoder.clear_depth(&target.depth, 1.0);
    }
    pub fn draw_text(&mut self, text: &str, pos: Pt3, color: [f32; 3]) {
        // for now we are discarding z because gfx_text can't handle it
        // TODO write better text renderer
        self.texts.push(text, [pos.x, pos.y], color);
    }
    pub fn draw_rect(&mut self, rect: Rect3, color: [f32; 3]) {
        self.rects.push(ColoredRect {
            translate: rect.pos.into(),
            scale: rect.size.into(),
            color,
        });
    }
    pub fn draw_textured_rect(&mut self, rect: Rect3, texture: ShaderResourceView<gl::Resources, [f32; 4]>) {
        self.textured_rects.push(rect, texture);
    }
    pub fn draw_pipe(&mut self, points: &[Pt3]) {
        self.pipes.push(points);
    }
    pub fn factory(&self) -> &gl::Factory {
        &self.factory
    }
    pub fn end_frame(&mut self, device: &mut gl::Device, target: &Target) {
        // XXX warning: draw order is still (somewhat) important.
        // in particular we rely on pipes being last
        self.rects.draw(&mut self.encoder, target);
        self.textured_rects.draw(&mut self.encoder, target);
        self.texts.draw(&mut self.encoder, target);
        self.pipes.draw(&mut self.encoder, target);
        self.encoder.flush(device);
    }
}

impl Clone for RenderContext {
    fn clone(&self) -> RenderContext {
        let mut factory = self.factory.clone();
        let encoder = factory.create_command_buffer().into();
        RenderContext {
            factory: factory,
            encoder: encoder,
            rects: self.rects.clone(),
            textured_rects: self.textured_rects.clone(),
            texts: self.texts.clone(),
            pipes: self.pipes.clone(),
        }
    }
}

pub struct TextureTarget {
    ctx: RenderContext,
    target_resource: ShaderResourceView<gl::Resources, [f32; 4]>,
    target: Target,
    size: Pt2,
}

impl TextureTarget {
    pub fn new(ctx: RenderContext, size: Pt2) -> TextureTarget {
        let mut factory = ctx.factory().clone();
        assert!(size.x >= 1.0 && size.y >= 1.0 && size.x <= 65535.0 && size.y <= 65535.0);
        let target_texture = factory
            .create_texture(
                texture::Kind::D2(size.x as u16, size.y as u16, texture::AaMode::Single),
                1, //levels
                Bind::RENDER_TARGET | Bind::SHADER_RESOURCE,
                Usage::Data,
                Some(gfx::format::ChannelType::Unorm),
            )
            .unwrap();
        let color_target = factory
            .view_texture_as_render_target(
                &target_texture,
                0,    //level
                None, //layer
            )
            .unwrap();
        let target_resource = factory
            .view_texture_as_shader_resource::<gfx::format::Rgba8>(
                &target_texture,
                (0, 0), // levels
                gfx::format::Swizzle::new(),
            )
            .unwrap();
        let depth_texture = factory
            .create_texture(
                texture::Kind::D2(size.x as u16, size.y as u16, texture::AaMode::Single),
                1, //levels
                Bind::DEPTH_STENCIL,
                Usage::Data,
                Some(gfx::format::ChannelType::Unorm),
            )
            .unwrap();
        let depth_target = factory
            .view_texture_as_depth_stencil(
                &depth_texture,
                0,    //level
                None, //layer
                texture::DepthStencilFlags::empty(),
            )
            .unwrap();
        let target = Target {
            color: color_target,
            depth: depth_target,
        };
        TextureTarget {
            ctx,
            target_resource,
            target,
            size,
        }
    }
    pub fn ctx(&mut self) -> &mut RenderContext {
        &mut self.ctx
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn shader_resource(&self) -> &ShaderResourceView<gl::Resources, [f32; 4]> {
        &self.target_resource
    }
    pub fn begin_frame(&mut self) {
        self.ctx.begin_frame(&self.target);
    }
    pub fn end_frame(&mut self, device: &mut gl::Device) {
        self.ctx.end_frame(device, &self.target);
    }
    pub fn size(&self) -> Pt2 {
        self.size
    }
}

fn target_dimensions(target: &Target) -> [f32; 2] {
    let dims = target.color.get_dimensions();
    [dims.0 as f32, dims.1 as f32]
}
