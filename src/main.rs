use drm::{
    control::{
        connector::{Handle as ConnectorHandle, Info as ConnectorInfo, State as ConnectorState},
        encoder::Handle as EncoderHandle,
        Device as ControlDevice, Mode, ModeTypeFlags, ResourceHandles,
    },
    Device,
};
use gbm::{BufferObjectFlags, Device as GbmDevice, Format as BufferFormat};
use std::os::fd::{AsFd, BorrowedFd};

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
    pub fn open(path: &str) -> Self {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        options.write(true);
        Card(options.open(path).unwrap())
    }
}

fn print_connector_info(gpu: &Card, resources: &ResourceHandles) {
    println!("Connectors:");

    for connector_handle in &resources.connectors {
        let force_probe = false;
        let connector = gpu
            .get_connector(*connector_handle, force_probe)
            .expect("Failed to get GPU connector info");

        let connector_interface = connector.interface().as_str();
        let interface_id = connector.interface_id();

        let connection_state = match connector.state() {
            ConnectorState::Connected => "✅",
            ConnectorState::Disconnected => "❌",
            ConnectorState::Unknown => "❔",
        };

        println!("\t{connector_interface}-{interface_id}, Connected={connection_state}");
    }
}

fn first_connected_connector(gpu: &Card) -> Option<ConnectorHandle> {
    gpu.resource_handles()
        .expect("Failed to get GPU resource handles")
        .connectors
        .iter()
        .find(|connector_handle| {
            let force_probe = false;
            gpu.get_connector(**connector_handle, force_probe)
                .expect("Failed to get GPU connector info")
                .state()
                == ConnectorState::Connected
        })
        .copied()
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

fn main() {
    // TODO(bschwind) - Use libdrm to iterate over available DRM devices.
    let gpu = Card::open("/dev/dri/card0");
    dbg!(gpu.get_driver().expect("Failed to get GPU driver info"));
    dbg!(gpu.get_bus_id().expect("Failed to get GPU bus ID"));

    let resources = gpu.resource_handles().expect("Failed to get GPU resource handles");

    print_connector_info(&gpu, &resources);

    let force_probe = false;
    let Some(first_connector_handle) = first_connected_connector(&gpu) else {
        println!("No display connected, exiting");
        return;
    };

    let first_connector = gpu
        .get_connector(first_connector_handle, force_probe)
        .expect("Failed to get GPU connector info");

    let connector_interface = first_connector.interface().as_str();
    let interface_id = first_connector.interface_id();

    println!("Using connector: {connector_interface}-{interface_id}");

    let Some(preferred_mode) = connector_preferred_mode(&first_connector) else {
        println!("No preferred mode for the first connected connector, exiting");
        return;
    };

    println!("Using mode: {preferred_mode:?}");

    let Some(encoder_handle) = first_encoder(&first_connector) else {
        println!("First connector does not have an encoder, exiting");
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

    let usage = BufferObjectFlags::SCANOUT | BufferObjectFlags::WRITE;
    use BufferFormat::*;
    for format in [
        Abgr8888, Argb8888, Bgr888, Bgra8888, Bgrx8888, Rgb888, Rgb888_a8, Rgba8888, Rgbx8888,
        Xbgr8888, Xrgb8888,
    ] {
        println!(
            "Is {} format supported for {:?} usage: {}",
            format,
            usage,
            gbm.is_format_supported(format, usage)
        );
    }

    let format = Argb8888;
    println!("gbm.create_buffer_object(width: {width}, height: {height}, format: {format}, usage: {usage:?})");
    let mut buffer_object = gbm.create_buffer_object::<()>(width, height, format, usage).unwrap();

    let buffer_data = vec![255u8; (width * height * 4) as usize];

    buffer_object.write(&buffer_data).unwrap().unwrap();

    let depth_bits = 32;
    let bits_per_pixel = 32;
    let fb = gbm.add_framebuffer(&buffer_object, depth_bits, bits_per_pixel).unwrap();

    gbm.set_crtc(crtc_handle, Some(fb), (0, 0), &[first_connector_handle], Some(preferred_mode))
        .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(5));
}
