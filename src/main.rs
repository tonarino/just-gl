use clap::Parser;
use drm::{
    control::{
        connector::Info as ConnectorInfo, crtc::Info as CrtcInfo,
        framebuffer::Handle as FramebufferHandle, Device as ControlDevice, Mode, PageFlipFlags,
    },
    Device,
};
use gbm::{BufferObjectFlags, Device as GbmDevice, Format as BufferFormat, Surface};
use just_gl::{
    connector_preferred_mode, first_encoder, get_connected_connectors, get_connector_name,
    print_connector_info, Card,
};
use std::path::PathBuf;

mod glutin;

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

struct DrmDisplay {
    gbm_device: GbmDevice<Card>,
    crtc: CrtcInfo,
    connector: ConnectorInfo,
    mode: Mode,
    width: u32,
    height: u32,
}

impl DrmDisplay {
    fn new(args: &Args) -> Option<DrmDisplay> {
        // TODO(bschwind) - Use libdrm to iterate over available DRM devices.
        let gpu = Card::open(&args.card_path);
        dbg!(gpu.get_driver().expect("Failed to get GPU driver info"));
        dbg!(gpu.get_bus_id().expect("Failed to get GPU bus ID"));

        let resources = gpu.resource_handles().expect("Failed to get GPU resource handles");

        print_connector_info(&gpu, &resources);

        let connector = {
            let mut connectors = get_connected_connectors(&gpu);
            let card_path = args.card_path.display();
            if let Some(name) = &args.connector {
                connectors.find(|info| &get_connector_name(info) == name).unwrap_or_else(|| {
                    panic!(
                    "Connector {name} does not exist or is not connected to {card_path}, exiting"
                )
                })
            } else {
                connectors
                    .next()
                    .unwrap_or_else(|| panic!("No connector connected to {card_path}, exiting."))
            }
        };

        let connector_interface = connector.interface().as_str();
        let interface_id = connector.interface_id();

        println!("Using connector: {connector_interface}-{interface_id}");

        let Some(mode) = connector_preferred_mode(&connector) else {
            println!("No preferred mode for the selected connector, exiting");
            return None;
        };

        println!("Using mode: {mode:?}");

        let Some(encoder_handle) = first_encoder(&connector) else {
            println!("Selected connector does not have an encoder, exiting");
            return None;
        };

        let encoder =
            gpu.get_encoder(encoder_handle).expect("Failed to get encoder from encoder handle");
        dbg!(encoder);

        let crtc_handle = *resources
            .filter_crtcs(encoder.possible_crtcs())
            .first()
            .expect("No CRTCs found for encoder");
        let crtc = gpu.get_crtc(crtc_handle).expect("Failed to get CRTC from CRTC handle");
        dbg!(crtc);

        let (width, height) = mode.size();
        let (width, height) = (width as u32, height as u32);
        let gbm_device = GbmDevice::new(gpu).expect("Failed to create GbmDevice");
        Some(DrmDisplay { gbm_device, crtc, connector, mode, width, height })
    }

    fn set_mode_with_framebuffer(&self, fb: Option<FramebufferHandle>) {
        self.gbm_device
            .set_crtc(self.crtc.handle(), fb, (0, 0), &[self.connector.handle()], Some(self.mode))
            .expect("set_crtc failed");
    }

    fn page_flip(&self, fb: FramebufferHandle) {
        let flags = PageFlipFlags::empty();
        let target_sequence = None;
        self.gbm_device
            .page_flip(self.crtc.handle(), fb, flags, target_sequence)
            .expect("Failed to flip page");
    }
}

struct Window {
    gbm_surface: Surface<FramebufferHandle>,
    drm_display: DrmDisplay,
    crtc_set: bool,
}

impl Window {
    fn new(drm_display: DrmDisplay) -> Window {
        let format = BufferFormat::Argb8888;
        // NOTE(mbernat): nvidia driver does not implement `create_surface`
        // and presumably one should use this variant instead. But the program crashes later
        // anyway when creating buffers with "Invalid argument" (22) kernel error.
        /*
        let modifiers = std::iter::once(Modifier::Linear);
        let gbm_surface: Surface<FramebufferHandle> = drm_display
            .gbm_device
            .create_surface_with_modifiers(drm_display.width, drm_display.height, format, modifiers)
            .unwrap();
        */

        let usage = BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING;
        let gbm_surface: Surface<FramebufferHandle> = drm_display
            .gbm_device
            .create_surface(drm_display.width, drm_display.height, format, usage)
            .unwrap();

        Window { gbm_surface, drm_display, crtc_set: false }
    }

    // TODO(mbernat): Add a "Frame" abstraction that calls `swap_buffers` internally when
    // it's finished (just like glium's Frame does) so that users don't need to bother with this.

    // SAFETY: this must be called exactly once after `eglSwapBuffers`,
    // which happens e.g. in `Frame::finish()`.
    unsafe fn swap_buffers(&mut self) {
        // TODO(mbernat): move this elsewhere
        let depth_bits = 24;
        let bits_per_pixel = 32;

        // SAFETY: we offloaded the `lock_front_buffer()` precondition to our caller
        let mut buffer_object = unsafe { self.gbm_surface.lock_front_buffer().unwrap() };

        // NOTE(mbernat): Frame buffer recycling:
        // we store an FB handle in buffer object's user_data() and reuse the FB when it exists
        let data = buffer_object.userdata().expect("Could not get buffer object user data");
        let fb = if let Some(handle) = data {
            *handle
        } else {
            let fb = self
                .drm_display
                .gbm_device
                .add_framebuffer(&buffer_object, depth_bits, bits_per_pixel)
                .unwrap();
            buffer_object.set_userdata(fb).expect("Could not set buffer object user data");
            fb
        };

        if !self.crtc_set {
            self.crtc_set = true;
            self.drm_display.set_mode_with_framebuffer(Some(fb));
        } else {
            self.drm_display.page_flip(fb);
        }
    }

    fn restore_original_display(&self) {
        self.drm_display.set_mode_with_framebuffer(self.drm_display.crtc.framebuffer());
    }
}

mod rwh_impl {
    /* SAFETY NOTICE
    Safety of these implementations is not enforced statically, it just happens to be the case
    right now because we control everything. If we were providing this code as a library the user
    could easily drop the display or window and then try rendering to them.

    To make this safer, one should tie together window's and handle's lifetimes.
    I believe raw-window-handle 0.6 does that by providing safe versions of these traits [1], [2].
    Unfortunately, glutin 0.30 uses rwh version 0.5.

    [1] https://docs.rs/raw-window-handle/0.6.0/raw_window_handle/trait.HasDisplayHandle.html
    [2] https://docs.rs/raw-window-handle/0.6.0/raw_window_handle/trait.HasWindowHandle.html
    */

    use super::Window;
    use gbm::AsRaw;
    use raw_window_handle::*;

    // SAFETY: surface is valid for the duration of the program
    unsafe impl HasRawWindowHandle for Window {
        fn raw_window_handle(&self) -> RawWindowHandle {
            let mut handle = GbmWindowHandle::empty();
            handle.gbm_surface = self.gbm_surface.as_raw() as *mut _;
            RawWindowHandle::Gbm(handle)
        }
    }

    // SAFETY: device is valid for the duration of the program
    unsafe impl HasRawDisplayHandle for Window {
        fn raw_display_handle(&self) -> RawDisplayHandle {
            let mut handle = GbmDisplayHandle::empty();
            handle.gbm_device = self.drm_display.gbm_device.as_raw() as *mut _;
            RawDisplayHandle::Gbm(handle)
        }
    }
}

fn main() {
    let args = Args::parse();
    let drm_display = DrmDisplay::new(&args).unwrap();
    let mut window = Window::new(drm_display);
    let glium_display = glutin::init(&window);

    use glium::Surface;
    let refresh_rate = 60;
    let frame_duration = 1.0 / refresh_rate as f64;
    let count = refresh_rate;
    let now = std::time::SystemTime::now();
    for i in 0..count {
        let ratio = i as f32 / count as f32;
        let mut frame = glium_display.draw();
        frame.clear_color(0.2 * ratio, 0.0, 0.5, 1.0);
        frame.finish().unwrap();
        // SAFETY: eglSwapBuffers is called by `frame.finish()`
        unsafe { window.swap_buffers() };
        std::thread::sleep(std::time::Duration::from_secs_f64(frame_duration));
    }
    println!("Duration: {:?}", std::time::SystemTime::now().duration_since(now));

    // NOTE(mbernat): It would be nice to invoke this in Window's drop method but the function
    // can panic and gbm_device is not UnwindSafe, so even catch_unwind doesn't help.
    window.restore_original_display();
}
