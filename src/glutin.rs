use std::num::NonZeroU32;

use glutin::{context::NotCurrentGlContextSurfaceAccessor, display::GetGlDisplay};
use raw_window_handle::HasRawWindowHandle;

pub(crate) fn init(
    window: &crate::Window,
) -> glium::backend::glutin::Display<glium::glutin::surface::WindowSurface> {
    use glutin::config::GlConfig;
    use glutin::display::GlDisplay;
    use glutin::{
        api::egl::display::Display,
        config::{ConfigSurfaceTypes, ConfigTemplateBuilder},
    };
    use raw_window_handle::HasRawDisplayHandle;

    let display_handle = window.raw_display_handle();
    let display = unsafe { Display::new(display_handle) }.expect("Failed to create glutin display");

    let config_template = ConfigTemplateBuilder::default()
        .with_alpha_size(8)
        .with_surface_type(ConfigSurfaceTypes::WINDOW)
        .build();

    let config = unsafe { display.find_configs(config_template) }
        .unwrap()
        .reduce(|config, acc| if config.num_samples() > acc.num_samples() { config } else { acc })
        .expect("No available configs");

    println!("Picked a config with {} samples", config.num_samples());

    // TODO(mbernat): get size from window
    // let (width, height): (u32, u32) = window.inner_size().into();
    let (width, height) = (1920, 1080);
    let attrs = glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
        .build(
            window.raw_window_handle(),
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );

    // Finally we can create a Surface, use it to make a PossiblyCurrentContext and create the glium Display
    // TODO(mbernat): make sure we're using the right config, this one comes from non-surface glutin example
    let surface = unsafe { config.display().create_window_surface(&config, &attrs).unwrap() };
    // NOTE(mbernat): None window handle should be fine for GBM
    let context_attributes = glutin::context::ContextAttributesBuilder::new().build(None);
    let current_context = (unsafe {
        config
            .display()
            .create_context(&config, &context_attributes)
            .expect("failed to create context")
    })
    .make_current(&surface)
    .unwrap();
    glium::backend::glutin::Display::from_context_surface(
        glutin::context::PossiblyCurrentContext::Egl(current_context),
        glutin::surface::Surface::Egl(surface),
    )
    .unwrap()
}
