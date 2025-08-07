// extern crate rscam;

// use rscam::{Camera, ResolutionInfo};

// fn main() {
//     let camera = Camera::new("/dev/video0").unwrap();

//     for wformat in camera.formats() {
//         let format = wformat.unwrap();
//         println!("{:?}", format);

//         let resolutions = camera.resolutions(&format.format).unwrap();

//         if let ResolutionInfo::Discretes(d) = resolutions {
//             for resol in &d {
//                 println!(
//                     "  {}x{}  {:?}",
//                     resol.0,
//                     resol.1,
//                     camera.intervals(&format.format, *resol).unwrap()
//                 );
//             }
//         } else {
//             println!("  {:?}", resolutions);
//         }
//     }
// }

use std::fs;
use std::io::Write;

fn main() {
    let mut camera = rscam::new("/dev/video0").unwrap();

    for wformat in camera.formats() {
        let format = wformat.unwrap();
        println!("{:?}", format);
        println!("  {:?}", camera.resolutions(format.format).unwrap());
    }

    camera
        .start(&rscam::Config {
            interval: (1, 10),
            resolution: (1280, 720),
            format: b"MJPG",
            ..Default::default()
        })
        .unwrap();

    for i in 0..10 {
        let frame = camera.capture().unwrap();

        println!("Frame of length {}", frame.len());

        let mut file = fs::File::create(&format!("frame-{}.jpg", i)).unwrap();
        file.write_all(&frame[..]).unwrap();
    }
}
