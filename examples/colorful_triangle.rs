use clap::Parser;
use just_gl::triangle::Triangle;
use std::path::PathBuf;

const DEFAULT_CARD_PATH: &str = "/dev/dri/card0";

#[derive(Parser)]
struct Args {
    /// Device file representing the GPU
    #[arg(long, default_value = DEFAULT_CARD_PATH)]
    card_path: PathBuf,

    /// Connector to use, e.g. DP-1; if not provided some connected one will be selected
    #[arg(long)]
    connector: Option<String>,
}

fn main() {
    let args = Args::parse();
    let drm_display =
        just_gl::drm::DrmDisplay::new(&args.card_path, args.connector.as_ref()).unwrap();
    let mut window = just_gl::window::Window::new(drm_display);
    let glium_display = just_gl::gl::init(&window);
    let mut triangle = Triangle::new(&glium_display);

    let count = 60;
    for i in 0..count {
        window.draw(|| {
            use glium::Surface;
            let ratio = i as f32 / count as f32;
            let mut frame = glium_display.draw();
            frame.clear_color(0.2 * ratio, 0.0, 0.5, 1.0);
            triangle.draw(&mut frame);
            frame.finish().unwrap();
        });
    }

    window.restore_original_display();
}
