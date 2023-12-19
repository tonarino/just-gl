use std::num::NonZeroU32;

use glium::backend::glutin::Display as GliumDisplay;
use glutin::config::{ConfigSurfaceTypes, ConfigTemplateBuilder};
use glutin::context::ContextAttributesBuilder;
use glutin::display::GlDisplay;
use glutin::surface::SurfaceAttributesBuilder;
use glutin::surface::WindowSurface;
use glutin::{context::NotCurrentGlContextSurfaceAccessor, display::GetGlDisplay};
use raw_window_handle::HasRawDisplayHandle;
use raw_window_handle::HasRawWindowHandle;

// NOTE(mbernat): This is a Frankenstein glutin/glium init function that I created because it's the easiest
// approach I found, not being familiar with either create. We should probably get rid of the glium bits here.
pub(crate) fn init(window: &crate::Window) -> GliumDisplay<WindowSurface> {
    // TODO(mbernat): Check unsafe usage in this function

    let display_handle = window.raw_display_handle();
    let display = unsafe {
        glutin::display::Display::new(display_handle, glutin::display::DisplayApiPreference::Egl)
    }
    .expect("Could not create glutin display");

    // TODO(mbernat): make sure we're using the right config
    let config_template =
        ConfigTemplateBuilder::default().with_surface_type(ConfigSurfaceTypes::WINDOW).build();

    let config = unsafe { display.find_configs(config_template) }
        .unwrap()
        .next()
        .expect("No available configs");

    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        window.raw_window_handle(),
        NonZeroU32::new(window.drm_display.width).unwrap(),
        NonZeroU32::new(window.drm_display.height).unwrap(),
    );

    // Finally we can create a Surface, use it to make a PossiblyCurrentContext and create the glium Display
    let surface = unsafe { config.display().create_window_surface(&config, &attrs).unwrap() };
    // NOTE(mbernat): None window handle should be fine for GBM
    let context_attributes = ContextAttributesBuilder::new().build(None);
    let no_context = unsafe {
        config
            .display()
            .create_context(&config, &context_attributes)
            .expect("failed to create context")
    };

    let current_context = no_context.make_current(&surface).unwrap();
    GliumDisplay::from_context_surface(current_context, surface).unwrap()
}
