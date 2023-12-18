use glutin::config::GlConfig;
use glutin::context::GlContext;
use glutin::display::GlDisplay;
use glutin::{
    api::egl::display::Display,
    config::{ConfigSurfaceTypes, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder},
};
use raw_window_handle::HasRawDisplayHandle;

pub(crate) fn init(window: &crate::Window) -> impl GlContext {
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

    // Context creation.
    // Using None handle should be okay for GBM
    let context_attributes = ContextAttributesBuilder::new().build(None);

    // Since glutin by default tries to create OpenGL core context, which may not be
    // present we should try gles.
    // Using None handle should be okay for GBM
    let fallback_context_attributes =
        ContextAttributesBuilder::new().with_context_api(ContextApi::Gles(None)).build(None);

    let not_current = unsafe {
        display.create_context(&config, &context_attributes).unwrap_or_else(|_| {
            display
                .create_context(&config, &fallback_context_attributes)
                .expect("failed to create context")
        })
    };

    // TODO(mbernat): use make_current() with surface
    not_current.make_current_surfaceless().unwrap()
}
