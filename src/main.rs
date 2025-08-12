use anyhow::Result;
use gstreamer as gst;
use gstreamer_app as gst_app;
use image::{Rgb, RgbImage};
use std::sync::Arc;
use std::time::Instant;

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    // r,g,b in 0..255 -> returns (h in 0..360, s in 0..1, v in 0..1)
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    let h = if delta == 0.0 {
        0.0
    } else if (max - rf).abs() < std::f32::EPSILON {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if (max - gf).abs() < std::f32::EPSILON {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };

    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { delta / max };
    let v = max;
    (h, s, v)
}

fn is_red_hsv(h: f32, s: f32, v: f32) -> bool {
    // Tune thresholds here:
    // Hue near 0..10 or 350..360 (we check as 0..10 and 160..180 for typical 0..360 wrap)
    // But camera hue ranges vary; you may adjust.
    let sat_min = 0.35; // require some saturation
    let val_min = 0.15; // exclude very dark pixels

    if s < sat_min || v < val_min {
        return false;
    }
    // Consider red hue ranges:
    (h >= 0.0 && h <= 15.0) || (h >= 345.0 && h <= 360.0)
}

fn annotate_bbox(img: &mut RgbImage, x0: u32, y0: u32, x1: u32, y1: u32) {
    // Draw a 3-pixel-thick rectangle in green
    let w = img.width();
    let h = img.height();
    for dx in 0..=2 {
        // top
        for x in x0.saturating_sub(dx)..=x1.saturating_add(dx) {
            if y0 + dx < h {
                img.put_pixel(
                    x.clamp(0, w - 1),
                    (y0 + dx).clamp(0, h - 1),
                    Rgb([0, 255, 0]),
                );
            }
        }
        // bottom
        for x in x0.saturating_sub(dx)..=x1.saturating_add(dx) {
            if y1 >= dx {
                img.put_pixel(
                    x.clamp(0, w - 1),
                    (y1 - dx).clamp(0, h - 1),
                    Rgb([0, 255, 0]),
                );
            }
        }
        // left
        for y in y0.saturating_sub(dx)..=y1.saturating_add(dx) {
            if x0 + dx < w {
                img.put_pixel(
                    (x0 + dx).clamp(0, w - 1),
                    y.clamp(0, h - 1),
                    Rgb([0, 255, 0]),
                );
            }
        }
        // right
        for y in y0.saturating_sub(dx)..=y1.saturating_add(dx) {
            if x1 >= dx {
                img.put_pixel(
                    (x1 - dx).clamp(0, w - 1),
                    y.clamp(0, h - 1),
                    Rgb([0, 255, 0]),
                );
            }
        }
    }
}

fn main() -> Result<()> {
    // Initialize GStreamer
    gst::init()?;

    // Desired capture size and caps
    let width = 640;
    let height = 480;

    // Try libcamerasrc pipeline; fallback to rpicamsrc if needed.
    // This appsink will produce RGB bytes (one pixel = 3 bytes: R, G, B)
    let pipeline_desc = format!(
        "libcamerasrc ! video/x-raw,width={w},height={h},format=RGB ! videoconvert ! appsink name=sink max-buffers=1 drop=true",
        w = width,
        h = height
    );

    // If libcamerasrc is not available on your system, replace "libcamerasrc" with "rpicamsrc"
    // e.g. "rpicamsrc ! video/x-raw,width=640,height=480,format=RGB ! ..."

    let pipeline = match gst::parse::launch(&pipeline_desc) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "Failed to create pipeline with libcamerasrc: {}. Trying rpicamsrc...",
                e
            );
            let alt = format!(
                "rpicamsrc ! video/x-raw,width={w},height={h},format=RGB ! videoconvert ! appsink name=sink max-buffers=1 drop=true",
                w = width,
                h = height
            );
            gst::parse::launch(&alt)?
        }
    };

    // Get the appsink
    let appsink = pipeline
        .clone()
        .dynamic_cast::<gst::Bin>()?
        .by_name("sink")
        .expect("appsink element named 'sink' missing")
        .downcast::<gst_app::AppSink>()
        .map_err(|_| anyhow::anyhow!("Failed to downcast to AppSink"))?;

    // Configure appsink to pull samples
    appsink.set_caps(Some(
        &gst::Caps::builder("video/x-raw")
            .field("format", &"RGB")
            .field("width", &(width as i32))
            .field("height", &(height as i32))
            .build(),
    ));

    pipeline.set_state(gst::State::Playing)?;

    println!("Started pipeline. Press Ctrl+C to stop.");

    let mut frame_count: u64 = 0;
    let start = Instant::now();

    loop {
        // Pull sample (blocking)
        if let Some(sample) = appsink.pull_sample() {
            frame_count += 1;
            let buffer = sample
                .buffer()
                .ok_or_else(|| anyhow::anyhow!("No buffer"))?;
            let map = buffer.map_readable()?;
            let data = map.as_slice();

            // data expected size = width * height * 3
            if data.len() < (width * height * 3) as usize {
                eprintln!("Warning: buffer too small: {} bytes", data.len());
                continue;
            }

            // Build an image::RgbImage from raw bytes (RGB)
            let img = RgbImage::from_raw(width, height, data.to_vec())
                .ok_or_else(|| anyhow::anyhow!("Failed to build image from raw bytes"))?;
            // We'll operate on a clone if we want to annotate
            let mut out_img = img.clone();

            // Scan pixels to find red mask, compute bounding box and centroid
            let mut min_x = width;
            let mut min_y = height;
            let mut max_x = 0u32;
            let mut max_y = 0u32;
            let mut count = 0u64;
            let mut sum_x = 0u64;
            let mut sum_y = 0u64;

            for y in 0..height {
                for x in 0..width {
                    let p = img.get_pixel(x, y);
                    let r = p[0];
                    let g = p[1];
                    let b = p[2];
                    let (h, s, v) = rgb_to_hsv(r, g, b);
                    if is_red_hsv(h, s, v) {
                        count += 1;
                        sum_x += x as u64;
                        sum_y += y as u64;
                        if x < min_x {
                            min_x = x;
                        }
                        if y < min_y {
                            min_y = y;
                        }
                        if x > max_x {
                            max_x = x;
                        }
                        if y > max_y {
                            max_y = y;
                        }

                        // Optionally mark mask-ish red pixel for debug (brighten)
                        out_img.put_pixel(
                            x,
                            y,
                            Rgb([255, (g / 2).saturating_add(50), (b / 2).saturating_add(50)]),
                        );
                    }
                }
            }

            // If found anything, compute centroid and bbox
            if count > 0 {
                let cx = (sum_x / count) as u32;
                let cy = (sum_y / count) as u32;

                println!(
                    "Frame {}: red pixels = {}, bbox = ({},{})-({},{}) centroid = ({},{})",
                    frame_count, count, min_x, min_y, max_x, max_y, cx, cy
                );

                // Annotate bounding box onto out_img
                annotate_bbox(&mut out_img, min_x, min_y, max_x, max_y);

                // Save annotated image for debugging occasionally
                if frame_count % 30 == 0 {
                    let filename = format!("/tmp/red_frame_{:05}.png", frame_count);
                    let _ = out_img.save(&filename);
                    println!("Saved annotated frame to {}", filename);
                }
            } else {
                println!("Frame {}: no red detected", frame_count);
            }

            // simple FPS print every 120 frames
            if frame_count % 120 == 0 {
                let elapsed = start.elapsed().as_secs_f32();
                println!(
                    "Processed {} frames in {:.2}s -> {:.2} FPS",
                    frame_count,
                    elapsed,
                    frame_count as f32 / elapsed
                );
            }
        } else {
            // appsink returned none => pipeline likely ended
            eprintln!("No sample (pipeline ended?)");
            break;
        }
    }

    pipeline.set_state(gst::State::Null)?;
    Ok(())
}
