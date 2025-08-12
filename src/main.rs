use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer_app as gst_app;
use opencv::{core, highgui, imgproc, prelude::*, types};
use std::time::Instant;

use gst::prelude::*;
use opencv::prelude::*;

// Helper: convert raw bytes -> OpenCV Mat (BGR)
fn mat_from_bgr_bytes(rows: i32, cols: i32, data: &[u8]) -> Result<Mat> {
    // Create a Mat from slice then reshape to rows x cols with 3 channels.
    // Mat::from_slice makes a single-row Mat; reshape will set proper rows and channels.
    let mut m = Mat::from_slice(data).context("Failed to create Mat from slice")?;
    // reshape(channels, rows)
    let m = m
        .reshape(3, rows)
        .context("Failed to reshape Mat into HxWx3")?;
    Ok(m)
}

fn main() -> Result<()> {
    // Initialize GStreamer
    gst::init().context("Failed to init gstreamer")?;

    // capture size
    let width = 640i32;
    let height = 480i32;

    // Build a pipeline asking for BGR (OpenCV default)
    let pipeline_desc = format!(
        "libcamerasrc ! video/x-raw,width={w},height={h},format=BGR ! videoconvert ! appsink name=sink max-buffers=1 drop=true",
        w = width,
        h = height
    );

    // Try libcamerasrc first, fallback to rpicamsrc
    let pipeline = match gst::parse::launch(&pipeline_desc) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "Failed to create pipeline with libcamerasrc: {}. Trying rpicamsrc...",
                e
            );
            let alt = format!(
                "rpicamsrc ! video/x-raw,width={w},height={h},format=BGR ! videoconvert ! appsink name=sink max-buffers=1 drop=true",
                w = width,
                h = height
            );
            gst::parse::launch(&alt).context("Failed to create pipeline with rpicamsrc")?
        }
    };

    // Get the appsink element
    let appsink = pipeline
        .clone()
        .dynamic_cast::<gst::Bin>()
        .map_err(|_| anyhow::anyhow!("Pipeline is not a Bin"))?
        .by_name("sink")
        .ok_or_else(|| anyhow::anyhow!("appsink element named 'sink' missing"))?
        .dynamic_cast::<gst_app::AppSink>()
        .map_err(|_| anyhow::anyhow!("Element 'sink' is not an AppSink"))?;

    // Set caps in case
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", &"BGR")
        .field("width", &(width as i32))
        .field("height", &(height as i32))
        .build();
    appsink.set_caps(Some(&caps));

    // Start pipeline
    pipeline
        .set_state(gst::State::Playing)
        .context("Failed to set pipeline to Playing")?;

    println!("Pipeline started. Opening window. Press 'q' in window or Ctrl+C to quit.");

    // OpenCV window
    let window_name = "Pi Camera - Red Detection";
    highgui::named_window(window_name, highgui::WINDOW_AUTOSIZE)?;

    let mut frame_count: u64 = 0;
    let t0 = Instant::now();

    loop {
        // pull_sample returns Result<Sample, BoolError>
        match appsink.pull_sample() {
            Ok(sample) => {
                frame_count += 1;
                let buffer = sample
                    .buffer()
                    .ok_or_else(|| anyhow::anyhow!("Sample has no buffer"))?;
                let map = buffer
                    .map_readable()
                    .context("Failed to map buffer readable")?;
                let data = map.as_slice();

                // Validate size
                let expected = (width * height * 3) as usize;
                if data.len() < expected {
                    eprintln!(
                        "Warning: buffer too small ({} < {}), skipping",
                        data.len(),
                        expected
                    );
                    continue;
                }

                // Build Mat from BGR bytes
                let mut mat = mat_from_bgr_bytes(height, width, &data[..expected])
                    .context("Failed to build Mat from raw bytes")?;

                // Convert to HSV
                let mut hsv = Mat::default();
                imgproc::cvt_color(&mat, &mut hsv, imgproc::COLOR_BGR2HSV, 0)?;

                // Threshold for red: two ranges because hue wraps
                // tune these values if necessary
                let lower_red1 = core::Scalar::new(0.0, 120.0, 70.0, 0.0);
                let upper_red1 = core::Scalar::new(10.0, 255.0, 255.0, 0.0);
                let lower_red2 = core::Scalar::new(170.0, 120.0, 70.0, 0.0);
                let upper_red2 = core::Scalar::new(180.0, 255.0, 255.0, 0.0);

                let mut mask1 = Mat::default();
                let mut mask2 = Mat::default();
                core::in_range(&hsv, &lower_red1, &upper_red1, &mut mask1)?;
                core::in_range(&hsv, &lower_red2, &upper_red2, &mut mask2)?;
                let mut mask = Mat::default();
                core::bitwise_or(&mask1, &mask2, &mut mask, &core::no_array()?)?;

                // Optional: morphological open/close to reduce noise
                let k = imgproc::get_structuring_element(
                    imgproc::MORPH_ELLIPSE,
                    core::Size::new(5, 5),
                    core::Point::new(-1, -1),
                )?;
                imgproc::morphology_ex(
                    &mask,
                    &mut mask,
                    imgproc::MORPH_OPEN,
                    &k,
                    core::Point::new(-1, -1),
                    1,
                    core::BORDER_DEFAULT,
                    core::Scalar::all(0.0),
                )?;
                imgproc::morphology_ex(
                    &mask,
                    &mut mask,
                    imgproc::MORPH_CLOSE,
                    &k,
                    core::Point::new(-1, -1),
                    1,
                    core::BORDER_DEFAULT,
                    core::Scalar::all(0.0),
                )?;

                // Find contours
                let mut contours = types::VectorOfVectorOfPoint::new();
                imgproc::find_contours(
                    &mask,
                    &mut contours,
                    imgproc::RETR_EXTERNAL,
                    imgproc::CHAIN_APPROX_SIMPLE,
                    core::Point::new(0, 0),
                )?;

                // Find largest contour by area
                let mut largest_area = 0.0;
                let mut best_rect = None;
                for i in 0..contours.len() {
                    let cnt = contours.get(i)?;
                    let area = imgproc::contour_area(&cnt, false)?;
                    if area > largest_area {
                        largest_area = area;
                        let rect = imgproc::bounding_rect(&cnt)?;
                        best_rect = Some(rect);
                    }
                }

                if let Some(r) = best_rect {
                    // Draw bounding box and centroid on mat
                    imgproc::rectangle(
                        &mut mat,
                        r,
                        core::Scalar::new(0.0, 255.0, 0.0, 0.0),
                        2,
                        imgproc::LINE_8,
                        0,
                    )?;
                    let cx = r.x + r.width / 2;
                    let cy = r.y + r.height / 2;
                    imgproc::circle(
                        &mut mat,
                        core::Point::new(cx, cy),
                        5,
                        core::Scalar::new(255.0, 0.0, 0.0, 0.0),
                        -1,
                        imgproc::LINE_8,
                        0,
                    )?;
                    println!(
                        "Frame {}: detected red object area {:.1} bbox x={} y={} w={} h={}",
                        frame_count, largest_area, r.x, r.y, r.width, r.height
                    );
                } else {
                    // optionally print no detection
                    // println!("Frame {}: no red detected", frame_count);
                }

                // Show the annotated frame
                highgui::imshow(window_name, &mat)?;
                // waitKey(1) needed to refresh window and handle events
                let key = highgui::wait_key(1)?;
                if key == 113 || key == 81 {
                    // 'q' or 'Q' to quit
                    println!("Quit key pressed. Exiting.");
                    break;
                }

                // Periodically print FPS
                if frame_count % 120 == 0 {
                    let elapsed = t0.elapsed().as_secs_f32();
                    println!(
                        "Processed {} frames in {:.2}s -> {:.2} FPS",
                        frame_count,
                        elapsed,
                        frame_count as f32 / elapsed
                    );
                }

                if frame_count == 361 {
                    break;
                }
            }
            Err(e) => {
                eprintln!("Error pulling sample from appsink: {}", e);
                // small sleep to avoid busy loop on errors
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    // cleanup
    pipeline.set_state(gst::State::Null)?;
    Ok(())
}
