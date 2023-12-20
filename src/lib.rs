use drm::control::{
    connector::{Info as ConnectorInfo, State as ConnectorState},
    encoder::Handle as EncoderHandle,
    Device as ControlDevice, Mode, ModeTypeFlags, ResourceHandles,
};
use std::{
    os::fd::{AsFd, BorrowedFd},
    path::Path,
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
