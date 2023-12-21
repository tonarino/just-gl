use clap::Parser;
use just_gl::gl;
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
    let glium_display = gl::init(&window);

    let count = 60;
    let now = std::time::SystemTime::now();
    for i in 0..count {
        // SAFETY: We call `frame.finish` as the last step
        unsafe {
            window.draw(|| {
                use glium::Surface;
                let ratio = i as f32 / count as f32;
                let mut frame = glium_display.draw();
                frame.clear_color(0.2 * ratio, 0.0, 0.5, 1.0);
                let mut triangle = just_gl::triangle::Triangle::new(&glium_display);
                triangle.draw(&mut frame);
                frame.finish().unwrap();
            })
        };
    }
    let duration = std::time::SystemTime::now().duration_since(now).unwrap().as_secs_f32();
    let fps = count as f32 / duration;
    println!("Rendered {count} frames at {fps} FPS");

    // NOTE(mbernat): This seems to be help on my laptop but it makes things worse on PragueRipper
    window.restore_original_display();
}
