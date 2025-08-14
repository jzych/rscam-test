use anyhow::Result;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use opencv::core::{self, CV_8UC3, Mat, Mat_AUTO_STEP};
use opencv::{highgui, imgproc};
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

    // Hardware setup. Try libcamera first, fallback to v4l2.
    let pipeline_desc =
        "libcamerasrc ! video/x-raw,width=640,height=480,format=BGR ! appsink name=sink";
    let pipeline = match gst::parse::launch(pipeline_desc) {
        Ok(p) => p,
        Err(_) => gst::parse::launch(
            "rpicamsrc ! video/x-raw,format=BGR,width=640,height=480 ! appsink name=sink",
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

        // // Mask for red
        // let mut mask1 = Mat::default();
        // let mut mask2 = Mat::default();
        // core::in_range(
        //     &hsv,
        //     &core::Scalar::new(0.0, 100.0, 100.0, 0.0),
        //     &core::Scalar::new(10.0, 255.0, 255.0, 0.0),
        //     &mut mask1,
        // )?;
        // core::in_range(
        //     &hsv,
        //     &core::Scalar::new(160.0, 100.0, 100.0, 0.0),
        //     &core::Scalar::new(179.0, 255.0, 255.0, 0.0),
        //     &mut mask2,
        // )?;

        // let mut mask = Mat::default();
        // core::bitwise_or(&mask1, &mask2, &mut mask, &core::no_array())?;

        // // Highlight red areas
        // let mut result = Mat::default();
        // core::bitwise_and(&bgr, &bgr, &mut result, &mask)?;

        // Show or save ever 100 frame
        if show_window {
            // Show window
            highgui::imshow("Camera Capture", &hsv)?;
            if highgui::wait_key(1)? == 27 {
                break;
            }
        } else {
            // Headless mode: save every 100 frames
            frame_count += 1;
            if frame_count % 100 == 0 {
                let filename = format!("/tmp/frame_{:06}.jpg", frame_count);
                imgcodecs::imwrite(&filename, &hsv, &core::Vector::new())?;
                println!("Saved {}", filename);
            }
        }
    }

    pipeline.set_state(gst::State::Null)?;
    Ok(())
}
