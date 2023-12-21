/// Taken from https://github.com/glium/glium/blob/master/examples/triangle.rs
use glium::index::PrimitiveType;
use glium::{implement_vertex, program, uniform, Display, Frame};
use glutin::surface::WindowSurface;

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}
implement_vertex!(Vertex, position, color);

pub struct Triangle {
    vertex_buffer: glium::VertexBuffer<Vertex>,
    index_buffer: glium::IndexBuffer<u16>,
    program: glium::Program,
}

impl Triangle {
    pub fn new(display: &Display<WindowSurface>) -> Self {
        let vertex_buffer = {
            glium::VertexBuffer::new(
                display,
                &[
                    Vertex { position: [-0.5, -0.5], color: [0.0, 1.0, 0.0] },
                    Vertex { position: [0.0, 0.5], color: [0.0, 0.0, 1.0] },
                    Vertex { position: [0.5, -0.5], color: [1.0, 0.0, 0.0] },
                ],
            )
            .unwrap()
        };

        // building the index buffer
        let index_buffer =
            glium::IndexBuffer::new(display, PrimitiveType::TrianglesList, &[0u16, 1, 2]).unwrap();

        // compiling shaders and linking them together
        let program = program!(display,
            100 => {
                vertex: "
                    #version 100

                    uniform lowp mat4 matrix;

                    attribute lowp vec2 position;
                    attribute lowp vec3 color;

                    varying lowp vec3 vColor;

                    void main() {
                        gl_Position = vec4(position, 0.0, 1.0) * matrix;
                        vColor = color;
                    }
                ",

                fragment: "
                    #version 100
                    varying lowp vec3 vColor;

                    void main() {
                        gl_FragColor = vec4(vColor, 1.0);
                    }
                ",
            },
        )
        .unwrap();

        Self { vertex_buffer, index_buffer, program }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        use glium::Surface;

        // For this example a simple identity matrix suffices
        let uniforms = uniform! {
            matrix: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0f32]
            ]
        };

        // Now we can draw the triangle
        frame
            .draw(
                &self.vertex_buffer,
                &self.index_buffer,
                &self.program,
                &uniforms,
                &Default::default(),
            )
            .unwrap();
    }
}
