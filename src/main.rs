use anyhow::Result;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use opencv::{core, highgui, imgproc, prelude::*};

fn main() -> Result<()> {
    // Init GStreamer and OpenCV GUI
    gst::init()?;
    highgui::named_window("Red Detection", highgui::WINDOW_AUTOSIZE)?;

    // Try libcamera first, fallback to v4l2
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

    let sink = pipeline
        .by_name("sink")
        .expect("Sink element not found")
        .dynamic_cast::<gst_app::AppSink>()
        .expect("Sink element is not an AppSink");

    pipeline.set_state(gst::State::Playing)?;

    loop {
        let sample = match sink.pull_sample() {
            Some(s) => s,
            None => continue,
        };

        let buffer = sample.buffer().expect("No buffer in sample");
        let map = buffer.map_readable().expect("Failed to map buffer");

        let caps = sample.caps().expect("No caps in sample");
        let s = caps.structure(0).expect("No structure in caps");

        let width = s.get::<i32>("width")?;
        let height = s.get::<i32>("height")?;

        let bgr_ref = Mat::from_slice(&map)?.reshape(3, height)?;
        let bgr = bgr_ref.try_clone()?; // make a real Mat

        // Convert to HSV
        let mut hsv = Mat::default();
        imgproc::cvt_color(&bgr, &mut hsv, imgproc::COLOR_BGR2HSV, 0)?;

        // Mask for red
        let mut mask1 = Mat::default();
        let mut mask2 = Mat::default();
        core::in_range(
            &hsv,
            &core::Scalar::new(0.0, 100.0, 100.0, 0.0),
            &core::Scalar::new(10.0, 255.0, 255.0, 0.0),
            &mut mask1,
        )?;
        core::in_range(
            &hsv,
            &core::Scalar::new(160.0, 100.0, 100.0, 0.0),
            &core::Scalar::new(179.0, 255.0, 255.0, 0.0),
            &mut mask2,
        )?;

        let mut mask = Mat::default();
        core::bitwise_or(&mask1, &mask2, &mut mask, &core::no_array())?;

        // Highlight red areas
        let mut result = Mat::default();
        core::bitwise_and(&bgr, &bgr, &mut result, &mask)?;

        // Show
        highgui::imshow("Red Detection", &result)?;
        if highgui::wait_key(1)? == 27 {
            break; // ESC key
        }
    }

    pipeline.set_state(gst::State::Null)?;
    Ok(())
}
