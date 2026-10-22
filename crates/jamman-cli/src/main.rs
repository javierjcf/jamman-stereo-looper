//! CLI `jamman`. De momento solo muestra lo conocido del dispositivo.

use jamman_core::{audio, device};

fn main() {
    println!(
        "JamMan Stereo — USB {:04x}:{:04x}, {} bucles por almacén, WAV {} Hz/{} bit/{} canales",
        device::USB_VENDOR_ID,
        device::USB_PRODUCT_ID,
        device::SLOTS_PER_STORAGE,
        audio::SAMPLE_RATE_HZ,
        audio::BITS_PER_SAMPLE,
        audio::CHANNELS,
    );
}
