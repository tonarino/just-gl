use drm::{
    control::{
        connector::{Info as ConnectorInfo, State as ConnectorState},
        crtc::Info as CrtcInfo,
        encoder::Handle as EncoderHandle,
        framebuffer::Handle as FramebufferHandle,
        Device as ControlDevice, Mode, ModeTypeFlags, PageFlipFlags, ResourceHandles,
    },
    Device,
};
use gbm::Device as GbmDevice;
use std::{
    os::fd::{AsFd, BorrowedFd},
    path::{Path, PathBuf},
};

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

pub fn get_connector_name(info: &ConnectorInfo) -> String {
    format!("{}-{}", info.interface().as_str(), info.interface_id())
}

pub fn print_connector_info(gpu: &Card, resources: &ResourceHandles) {
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

pub fn get_connected_connectors(gpu: &Card) -> impl Iterator<Item = ConnectorInfo> + '_ {
    let handles = gpu.resource_handles().expect("Failed to get GPU resource handles");
    handles.connectors.into_iter().filter_map(|connector_handle| {
        let force_probe = false;
        let info = gpu
            .get_connector(connector_handle, force_probe)
            .expect("Failed to get GPU connector info");
        (info.state() == ConnectorState::Connected).then_some(info)
    })
}

pub fn connector_preferred_mode(connector_info: &ConnectorInfo) -> Option<Mode> {
    connector_info
        .modes()
        .iter()
        .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
        .copied()
}

pub fn first_encoder(connector_info: &ConnectorInfo) -> Option<EncoderHandle> {
    connector_info.encoders().iter().next().copied()
}

pub struct DrmDisplay {
    pub(crate) gbm_device: GbmDevice<Card>,
    pub(crate) crtc: CrtcInfo,
    pub(crate) connector: ConnectorInfo,
    pub(crate) mode: Mode,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl DrmDisplay {
    pub fn new(card_path: &PathBuf, connector: Option<&String>) -> Option<DrmDisplay> {
        // TODO(bschwind) - Use libdrm to iterate over available DRM devices.
        let gpu = Card::open(card_path);
        dbg!(gpu.get_driver().expect("Failed to get GPU driver info"));
        dbg!(gpu.get_bus_id().expect("Failed to get GPU bus ID"));

        let resources = gpu.resource_handles().expect("Failed to get GPU resource handles");

        print_connector_info(&gpu, &resources);

        let connector = {
            let mut connectors = get_connected_connectors(&gpu);
            let card_path = card_path.display();
            if let Some(name) = connector {
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

    pub(crate) fn set_mode_with_framebuffer(&self, fb: Option<FramebufferHandle>) {
        self.gbm_device
            .set_crtc(self.crtc.handle(), fb, (0, 0), &[self.connector.handle()], Some(self.mode))
            .expect("set_crtc failed");
    }

    pub(crate) fn page_flip(&self, fb: FramebufferHandle) {
        let flags = PageFlipFlags::EVENT;
        let target_sequence = None;
        self.gbm_device
            .page_flip(self.crtc.handle(), fb, flags, target_sequence)
            .expect("Failed to schedule a page flip");
    }
}
