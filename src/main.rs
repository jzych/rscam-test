use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer_app as gst_app;
use opencv::{core, highgui, imgproc, prelude::*};
use std::time::Instant;

use gst::prelude::*;

/// Build an OpenCV Mat (rows x cols x 3 channels) from a raw BGR byte slice.
/// Expects data.len() >= (rows * cols * 3) and will use the first that many bytes.
fn mat_from_bgr_bytes(rows: i32, cols: i32, data: &[u8]) -> Result<Mat> {
    // Mat::from_slice builds a single-row Mat with (rows*cols*3) elements, then reshape into H x W with 3 channels
    let expected = (rows as usize) * (cols as usize) * 3;
    if data.len() < expected {
        anyhow::bail!(
            "Not enough data for requested size: {} < {}",
            data.len(),
            expected
        );
    }
    let slice = &data[..expected];
    let m = Mat::from_slice(slice).context("Failed to make Mat from slice")?;
    let m = m
        .reshape(3, rows)
        .context("Failed to reshape Mat to HxWx3")?;
    Ok(m)
}

fn main() -> Result<()> {
    // Initialize GStreamer
    gst::init().context("Failed to initialize GStreamer")?;

    // Desired resolution (tweak smaller for performance: 320x240)
    let width = 640i32;
    let height = 480i32;

    // Pipeline asking for BGR (so OpenCV expects BGR)
    let pipeline_desc = format!(
        "libcamerasrc ! video/x-raw,width={w},height={h},format=BGR ! videoconvert ! appsink name=sink max-buffers=1 drop=true",
        w = width,
        h = height
    );

    // Try libcamerasrc first, fallback to v4l2src (more widely available)
    let pipeline = match gst::parse::launch(&pipeline_desc) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "libcamerasrc pipeline failed: {}. Trying v4l2src fallback...",
                e
            );
            // Change device if necessary (e.g., /dev/video0)
            let alt = format!(
                "v4l2src device=/dev/video0 ! video/x-raw,width={w},height={h},format=BGR ! videoconvert ! appsink name=sink max-buffers=1 drop=true",
                w = width,
                h = height
            );
            gst::parse::launch(&alt).context("Failed to create pipeline with v4l2src")?
        }
    };

    // Retrieve appsink
    let appsink = pipeline
        .clone()
        .dynamic_cast::<gst::Bin>()
        .map_err(|_| anyhow::anyhow!("Pipeline is not a Bin"))?
        .by_name("sink")
        .ok_or_else(|| anyhow::anyhow!("appsink element named 'sink' missing"))?
        .dynamic_cast::<gst_app::AppSink>()
        .map_err(|_| anyhow::anyhow!("Element 'sink' is not an AppSink"))?;

    // Set caps to be explicit (optional but helpful)
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

    println!("Pipeline started. Window should open. Press 'q' in window to quit.");

    // Create OpenCV window
    let window_name = "Pi Camera - Red Highlight";
    highgui::named_window(window_name, highgui::WINDOW_AUTOSIZE)?;

    let mut frame_count: u64 = 0;
    let t0 = Instant::now();

    loop {
        // pull_sample returns Result<Sample, glib::BoolError>
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

                // Ensure data has expected size (BGR)
                let expected = (width * height * 3) as usize;
                if data.len() < expected {
                    eprintln!(
                        "Warning: buffer too small ({} < {}), skipping frame",
                        data.len(),
                        expected
                    );
                    continue;
                }

                // Create Mat from raw bytes (BGR)
                let mut mat = mat_from_bgr_bytes(height, width, &data[..expected])
                    .context("Failed to create Mat from bytes")?;

                // Convert to HSV
                let mut hsv = Mat::default();
                imgproc::cvt_color(&mat, &mut hsv, imgproc::COLOR_BGR2HSV, 0)
                    .context("Failed to convert to HSV")?;

                // Red ranges in HSV: two ranges because hue wraps
                let lower1 = core::Scalar::new(0.0, 120.0, 70.0, 0.0);
                let upper1 = core::Scalar::new(10.0, 255.0, 255.0, 0.0);
                let lower2 = core::Scalar::new(170.0, 120.0, 70.0, 0.0);
                let upper2 = core::Scalar::new(180.0, 255.0, 255.0, 0.0);

                let mut mask1 = Mat::default();
                let mut mask2 = Mat::default();
                core::in_range(&hsv, &lower1, &upper1, &mut mask1)?;
                core::in_range(&hsv, &lower2, &upper2, &mut mask2)?;

                // Combine masks
                let mut mask = Mat::default();
                core::bitwise_or(&mask1, &mask2, &mut mask, &core::no_array()?)?;

                // Optional: small morphology to reduce speckle (cheap)
                let kernel = imgproc::get_structuring_element(
                    imgproc::MORPH_RECT,
                    core::Size::new(3, 3),
                    core::Point::new(-1, -1),
                )?;
                imgproc::morphology_ex(
                    &mask,
                    &mut mask,
                    imgproc::MORPH_OPEN,
                    &kernel,
                    core::Point::new(-1, -1),
                    1,
                    core::BORDER_DEFAULT,
                    core::Scalar::all(0.0),
                )?;
                imgproc::morphology_ex(
                    &mask,
                    &mut mask,
                    imgproc::MORPH_CLOSE,
                    &kernel,
                    core::Point::new(-1, -1),
                    1,
                    core::BORDER_DEFAULT,
                    core::Scalar::all(0.0),
                )?;

                // Highlight: color the red pixels (make a colored overlay)
                let mut highlight = Mat::zeros(height, width, core::CV_8UC3)?.to_mat()?; // black image
                // Fill highlight with red color where mask is set
                highlight.set_to(&core::Scalar::new(0.0, 0.0, 255.0, 0.0), &mask)?;
                // Blend original and highlight
                let mut out = Mat::default();
                core::add_weighted(&mat, 0.7, &highlight, 0.3, 0.0, &mut out, -1)?;

                // Show
                highgui::imshow(window_name, &out)?;
                // wait_key(1) necessary to process events and update window
                let key = highgui::wait_key(1)?;
                if key == 113 || key == 81 {
                    // q or Q
                    println!("Quit requested. Exiting.");
                    break;
                }

                // occasional FPS log
                if frame_count % 120 == 0 {
                    let elapsed = t0.elapsed().as_secs_f32();
                    println!(
                        "Frames: {}  Time: {:.2}s  FPS: {:.2}",
                        frame_count,
                        elapsed,
                        frame_count as f32 / elapsed
                    );
                }
            }
            Err(e) => {
                eprintln!("Failed pulling sample from appsink: {}", e);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }

    // cleanup
    pipeline.set_state(gst::State::Null)?;
    Ok(())
}
