use std::env;
use std::fs::File;
use std::io::Read;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use gfx;
use gfx::texture;
use gfx::traits::{Factory, FactoryExt};
use gfx::{CommandBuffer, Device, Encoder, IntoIndexBuffer, PipelineState, Resources, Slice};
use gfx::memory::{Usage, Bind};
use gfx::buffer::Role;
use gfx::handle::{Buffer, DepthStencilView, RenderTargetView, Sampler, ShaderResourceView, Texture};
use gfx_window_glutin as gfx_glutin;
use gfx_text;
use gfx_device_gl as gl;

pub type ColorFormat = gfx::format::Rgba8;
pub type DepthFormat = gfx::format::DepthStencil;

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

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
        out: gfx::RenderTarget<ColorFormat> = "Target0",
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
        out: gfx::RenderTarget<ColorFormat> = "Target0",
        depth: gfx::DepthTarget<DepthFormat> = gfx::state::Depth {
            fun: gfx::state::Comparison::LessEqual,
            write: true,
        },
    }
}

#[derive(Copy, Clone)]
pub struct Rect {
    pub translate: [f32; 3],
    pub scale: [f32; 2],
}

pub struct Target {
    pub color: RenderTargetView<gl::Resources, ColorFormat>,
    pub depth: DepthStencilView<gl::Resources, DepthFormat>,
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> [Vertex; 4] {
    [
        Vertex { pos: [x, y] },
        Vertex { pos: [x, y + h] },
        Vertex {
            pos: [x + w, y + h],
        },
        Vertex { pos: [x + w, y] },
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
        let mut slice = Slice {
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
    rects: Vec<(Rect, ShaderResourceView<gl::Resources, [f32; 4]>)>,
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
    fn push(&mut self, rect: Rect, texture: ShaderResourceView<gl::Resources, [f32; 4]>) {
        self.rects.push((rect, texture));
    }
    fn draw(&mut self, encoder: &mut Encoder<gl::Resources, gl::CommandBuffer>, target: &Target) {
        for (rect, texture) in self.rects.drain(..) {
            let vertices = [
                TexturedVertex {
                    translate: [rect.translate[0], rect.translate[1], rect.translate[2]],
                    tex_coord: [0.0, 1.0],
                },
                TexturedVertex {
                    translate: [
                        rect.translate[0],
                        rect.translate[1] + rect.scale[1],
                        rect.translate[2],
                    ],
                    tex_coord: [0.0, 0.0],
                },
                TexturedVertex {
                    translate: [
                        rect.translate[0] + rect.scale[0],
                        rect.translate[1] + rect.scale[1],
                        rect.translate[2],
                    ],
                    tex_coord: [1.0, 0.0],
                },
                TexturedVertex {
                    translate: [
                        rect.translate[0] + rect.scale[0],
                        rect.translate[1],
                        rect.translate[2],
                    ],
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
    fn new(mut factory: gl::Factory) -> TextRenderer {
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
        renderer.draw(encoder, &target.color);
    }
}

pub struct RenderContext {
    factory: gl::Factory,
    encoder: Encoder<gl::Resources, gl::CommandBuffer>,
    rects: RectRenderer,
    textured_rects: TexturedRectRenderer,
    texts: TextRenderer,
}
impl RenderContext {
    pub fn new(mut factory: gl::Factory) -> RenderContext {
        let encoder = factory.create_command_buffer().into();
        let rects = RectRenderer::new(factory.clone());
        let textured_rects = TexturedRectRenderer::new(factory.clone());
        let texts = TextRenderer::new(factory.clone());

        RenderContext {
            factory,
            encoder,
            rects,
            textured_rects,
            texts,
        }
    }
    pub fn begin_frame(&mut self, target: &Target) {
        self.encoder.clear(&target.color, BLACK);
        self.encoder.clear_depth(&target.depth, 1.0);
    }
    pub fn draw_text(&mut self, text: &str, pos: [f32; 2], color: [f32; 3]) {
        self.texts.push(text, pos, color);
    }
    pub fn draw_rect(&mut self, rect: ColoredRect) {
        self.rects.push(rect);
    }
    pub fn draw_textured_rect(&mut self, rect: Rect, texture: ShaderResourceView<gl::Resources, [f32; 4]>) {
        self.textured_rects.push(rect, texture);
    }
    pub fn factory(&self) -> &gl::Factory {
        &self.factory
    }
    pub fn end_frame(&mut self, device: &mut gl::Device, target: &Target) {
        self.rects.draw(&mut self.encoder, target);
        self.textured_rects.draw(&mut self.encoder, target);
        self.texts.draw(&mut self.encoder, target);
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
        }
    }
}

pub fn point_in_rect(pos: [f32; 2], rect: &Rect) -> bool {
    pos[0] >= rect.translate[0] && pos[0] <= rect.translate[0] + rect.scale[0] && pos[1] >= rect.translate[1]
        && pos[1] <= rect.translate[1] + rect.scale[1]
}
fn pixels_to_coords(size: [f32; 2], pix: [f32; 2]) -> [f32; 2] {
    let aspect = size[0] / size[1];
    [
        aspect * (pix[0] / size[0] * 2.0 - 1.0),
        pix[1] / size[1] * -2.0 + 1.0,
    ]
}
fn coords_to_pixels(size: [f32; 2], coord: [f32; 2]) -> [f32; 2] {
    let aspect = size[0] / size[1];
    [
        (coord[0] / aspect * 0.5 + 0.5) * size[0],
        (-coord[1] * 0.5 + 0.5) * size[1],
    ]
}
fn target_dimensions(target: &Target) -> [f32; 2] {
    let dims = target.color.get_dimensions();
    [dims.0 as f32, dims.1 as f32]
}
fn aspect_ratio(size: [f32; 2]) -> f32 {
    size[0] / size[1]
}
