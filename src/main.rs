use clap::Parser;
use drm::{
    control::{
        connector::{Info as ConnectorInfo, State as ConnectorState},
        encoder::Handle as EncoderHandle,
        Device as ControlDevice, Mode, ModeTypeFlags, ResourceHandles,
    },
    Device,
};
use gbm::{BufferObjectFlags, Device as GbmDevice, Format as BufferFormat};
use std::{
    ffi::c_void,
    os::fd::{AsFd, BorrowedFd},
    path::{Path, PathBuf},
};

mod glutin;

/// A simple wrapper for a device node.
pub struct Card(std::fs::File);

/// Implementing [`AsFd`] is a prerequisite to implementing the traits found
/// in this crate. Here, we are just calling [`File::as_fd()`] on the inner
/// [`File`].
impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl drm::Device for Card {}
impl ControlDevice for Card {}

/// Simple helper methods for opening a `Card`.
impl Card {
    pub fn open(path: impl AsRef<Path>) -> Self {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        options.write(true);
        Card(options.open(path).unwrap())
    }
}

fn get_connector_name(info: &ConnectorInfo) -> String {
    format!("{}-{}", info.interface().as_str(), info.interface_id())
}

fn print_connector_info(gpu: &Card, resources: &ResourceHandles) {
    println!("Connectors:");

    for connector_handle in &resources.connectors {
        let force_probe = false;
        let connector = gpu
            .get_connector(*connector_handle, force_probe)
            .expect("Failed to get GPU connector info");

        let name = get_connector_name(&connector);

        let connection_state = match connector.state() {
            ConnectorState::Connected => "✅",
            ConnectorState::Disconnected => "❌",
            ConnectorState::Unknown => "❔",
        };

        println!("\t{name}, Connected={connection_state}");
    }
}

fn get_connected_connectors(gpu: &Card) -> impl Iterator<Item = ConnectorInfo> + '_ {
    let handles = gpu.resource_handles().expect("Failed to get GPU resource handles");
    handles.connectors.into_iter().filter_map(|connector_handle| {
        let force_probe = false;
        let info = gpu
            .get_connector(connector_handle, force_probe)
            .expect("Failed to get GPU connector info");
        (info.state() == ConnectorState::Connected).then_some(info)
    })
}

fn connector_preferred_mode(connector_info: &ConnectorInfo) -> Option<Mode> {
    connector_info
        .modes()
        .iter()
        .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
        .copied()
}

fn first_encoder(connector_info: &ConnectorInfo) -> Option<EncoderHandle> {
    connector_info.encoders().iter().next().copied()
}

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

struct Window {
    gbm_device: *mut c_void,
}

impl Window {
    fn new(gbm_device: *mut c_void) -> Window {
        Window { gbm_device }
    }
}

mod rwh_impl {
    use super::Window;
    use raw_window_handle::*;

    unsafe impl HasRawWindowHandle for Window {
        fn raw_window_handle(&self) -> RawWindowHandle {
            // alternative
            // RawWindowHandle::Drm()

            let mut handle = GbmWindowHandle::empty();
            handle.gbm_surface = std::ptr::null_mut();

            RawWindowHandle::Gbm(handle)
        }
    }

    unsafe impl HasRawDisplayHandle for Window {
        fn raw_display_handle(&self) -> RawDisplayHandle {
            // alternative
            // RawDisplayHandle::Drm();

            let mut handle = GbmDisplayHandle::empty();
            handle.gbm_device = self.gbm_device;
            RawDisplayHandle::Gbm(handle)
        }
    }
}

fn main() {
    let args = Args::parse();

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

    let Some(preferred_mode) = connector_preferred_mode(&connector) else {
        println!("No preferred mode for the selected connector, exiting");
        return;
    };

    println!("Using mode: {preferred_mode:?}");

    let Some(encoder_handle) = first_encoder(&connector) else {
        println!("Selected connector does not have an encoder, exiting");
        return;
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

    let (width, height) = preferred_mode.size();
    let (width, height) = (width as u32, height as u32);

    let gbm = GbmDevice::new(gpu).expect("Failed to create GbmDevice");
    let window = Window::new(gbm::AsRaw::as_raw(&gbm) as *mut _);

    let mut buffer_object = gbm
        .create_buffer_object::<()>(
            width,
            height,
            BufferFormat::Argb8888,
            BufferObjectFlags::SCANOUT | BufferObjectFlags::WRITE,
        )
        .unwrap();

    let buffer_data = vec![255u8; (width * height * 4) as usize];

    buffer_object.write(&buffer_data).unwrap().unwrap();

    let depth_bits = 32;
    let bits_per_pixel = 32;
    let fb = gbm.add_framebuffer(&buffer_object, depth_bits, bits_per_pixel).unwrap();

    let _glutin_context = glutin::init(&window);
    gbm.set_crtc(crtc_handle, Some(fb), (0, 0), &[connector.handle()], Some(preferred_mode))
        .unwrap();

    // std::thread::sleep(std::time::Duration::from_secs(5));
}
