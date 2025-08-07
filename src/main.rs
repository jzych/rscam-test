use std::io;

use v4l::context;
use v4l::prelude::*;

fn main() -> io::Result<()> {
    let devices = context::enum_devices();

    for dev in devices {
        println!("{}: {}", dev.index(), dev.name().unwrap());
    }

    let path = "/dev/video0";
    println!("Using device: {}\n", path);

    let dev = Device::with_path(path)?;
    println!("1");
    let controls = dev.query_controls()?;
    println!("2");

    for control in controls {
        println!("{}", control);
    }

    Ok(())
}
