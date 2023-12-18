use clap::Parser;
use drm::{control::Device as ControlDevice, Device};
use gbm::{BufferObjectFlags, Device as GbmDevice, Format as BufferFormat};
use just_gl::{
    connector_preferred_mode, first_encoder, get_connected_connectors, get_connector_name,
    print_connector_info, Card,
};
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

fn init_glutin() {
    use glutin::api::egl::device::Device;
    use glutin::api::egl::display::Display;
    let devices = Device::query_devices().expect("Failed to query devices").collect::<Vec<_>>();

    for (index, device) in devices.iter().enumerate() {
        println!(
            "Device {}: Name: {} Vendor: {}",
            index,
            device.name().unwrap_or("UNKNOWN"),
            device.vendor().unwrap_or("UNKNOWN")
        );
    }

    let device = devices.first().expect("No available devices");

    // Create a display using the device.
    let display = unsafe { Display::with_device(device, None) }.expect("Failed to create display");
}

fn main() {
    init_glutin();
    return;

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

    gbm.set_crtc(crtc_handle, Some(fb), (0, 0), &[connector.handle()], Some(preferred_mode))
        .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(5));
}
