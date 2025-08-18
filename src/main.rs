use anyhow::Result;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use opencv::core::{self, CV_8UC3, Mat, Mat_AUTO_STEP};
use opencv::{highgui, imgcodecs, imgproc};
use std::ffi::c_void;

fn main() -> Result<()> {
    // Parse command line args
    let args: Vec<String> = std::env::args().collect();
    let show_window = args.iter().any(|a| a == "-c");
    let mut frame_count = 0;

    // Init GStreamer and OpenCV GUI
    gst::init()?;
    if show_window {
        highgui::named_window("Camera Capture", highgui::WINDOW_AUTOSIZE)?; // While sshing -X flag is needed and enabled X11 forwarding
    }

    // Hardware setup. Try libcamera first, fallback to v4l2 for older OS.
    let pipeline_desc = "libcamerasrc ! videoconvert ! video/x-raw,format=BGR,width=640,height=480 ! appsink name=sink";
    let pipeline = match gst::parse::launch(pipeline_desc) {
        Ok(p) => p,
        Err(_) => gst::parse::launch(
            "rpicamsrc ! videoconvert ! video/x-raw,format=BGR,width=640,height=480 ! appsink name=sink",
        )?,
    };

    let pipeline = pipeline
        .dynamic_cast::<gst::Pipeline>()
        .expect("Expected a Pipeline");

    // Bridge between GStreamer and program. Raw frames can be pulled from it.
    let sink = pipeline
        .by_name("sink")
        .expect("Sink element not found")
        .dynamic_cast::<gst_app::AppSink>()
        .expect("Sink element is not an AppSink");

    // Starts camera capture
    pipeline.set_state(gst::State::Playing)?;

    loop {
        // Polling for next frame
        let sample = match sink.pull_sample() {
            Ok(s) => s,
            Err(_) => continue,
        };

        let buffer = sample.buffer().expect("No buffer in sample");
        let map = buffer.map_readable().expect("Failed to map buffer");

        let caps = sample.caps().expect("No caps in sample");
        let s = caps.structure(0).expect("No structure in caps");

        let width: i32 = s.get("width")?;
        let height: i32 = s.get("height")?;

        // Create a Mat that references the GStreamer buffer
        // Frame is 2D with width x height x channels
        // SAFETY: We are creating a Mat view into GStreamer’s memory buffer.
        // This is safe as long as `bgr` is not used after `map` goes out of scope.
        let bgr = unsafe {
            Mat::new_rows_cols_with_data_unsafe(
                height,
                width,
                CV_8UC3,
                map.as_ptr() as *mut c_void,
                Mat_AUTO_STEP,
            )?
        };

        // Convert to HSV
        let mut hsv = Mat::default();
        imgproc::cvt_color(&bgr, &mut hsv, imgproc::COLOR_BGR2HSV, 0)?;

        // Show or save ever 100 frame
        if show_window {
            // Show window
            highgui::imshow("Camera Capture", &hsv)?;
            if highgui::wait_key(1)? == 27 {
                break;
            }
        } else {
            // Headless mode: save every 100 frames
            if frame_count % 100 == 0 {
                println!("Caps: {:?}", s.to_string());
                let filename = format!("/tmp/frame_{:06}.jpg", frame_count);
                imgcodecs::imwrite(&filename, &hsv, &core::Vector::new())?;
                println!("Saved {}", filename);
            }
            frame_count += 1;
        }
    }

    pipeline.set_state(gst::State::Null)?;
    Ok(())
}
