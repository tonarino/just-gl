use drm::{
    control::{
        connector::{Handle as ConnectorHandle, Info as ConnectorInfo, State as ConnectorState},
        Device as ControlDevice, Mode, ModeTypeFlags, ResourceHandles,
    },
    Device,
};
use gbm::Device as GbmDevice;
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

fn main() {
    // TODO(bschwind) - Use libdrm to iterate over available DRM devices.
    let gpu = Card::open("/dev/dri/card0");
    dbg!(gpu.get_driver().expect("Failed to get GPU driver info"));
    dbg!(gpu.get_bus_id().expect("Failed to get GPU bus ID"));

    let resources = gpu.resource_handles().expect("Failed to get GPU resource handles");

    print_connector_info(&gpu, &resources);

    let force_probe = false;
    let Some(first_handle) = first_connected_connector(&gpu) else {
        println!("No display connected, exiting");
        return;
    };

    let first_connector =
        gpu.get_connector(first_handle, force_probe).expect("Failed to get GPU connector info");

    let connector_interface = first_connector.interface().as_str();
    let interface_id = first_connector.interface_id();

    println!("Using connector: {connector_interface}-{interface_id}");

    let Some(preferred_mode) = connector_preferred_mode(&first_connector) else {
        println!("No preferred mode for the first connected connector, exiting");
        return;
    };

    println!("Using mode: {preferred_mode:?}");

    let _gbm = GbmDevice::new(gpu).expect("Failed to create GbmDevice");
}
