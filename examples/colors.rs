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

    let refresh_rate = 60;
    let frame_duration = 1.0 / refresh_rate as f64;
    let count = 2;
    let now = std::time::SystemTime::now();
    for i in 0..count {
        window.draw(|| {
            use glium::Surface;
            let ratio = i as f32 / count as f32;
            let mut frame = glium_display.draw();
            frame.clear_color(0.2 * ratio, 0.0, 0.5, 1.0);
            frame.finish().unwrap();
        });
    }
    println!("Duration: {:?}", std::time::SystemTime::now().duration_since(now));

    // NOTE(mbernat): It would be nice to invoke this in Window's drop method but the function
    // can panic and gbm_device is not UnwindSafe, so even catch_unwind doesn't help.
    window.restore_original_display();
}
