use anyhow::Result;
use opencv::{core, highgui, imgproc, prelude::*, videoio};

fn main() -> Result<()> {
    // Adjust resolution for performance
    let width = 640;
    let height = 480;

    // GStreamer pipeline for Pi camera (libcamera)
    let pipeline = format!(
        "libcamerasrc ! video/x-raw,width={width},height={height},format=BGR ! videoconvert ! appsink"
    );

    let mut cap = videoio::VideoCapture::from_file(&pipeline, videoio::CAP_GSTREAMER)?;
    if !videoio::VideoCapture::is_opened(&cap)? {
        anyhow::bail!("Could not open camera with GStreamer pipeline: {pipeline}");
    }

    loop {
        let mut frame = Mat::default();
        cap.read(&mut frame)?;
        if frame.empty() {
            continue;
        }

        // Convert to HSV
        let mut hsv = Mat::default();
        imgproc::cvt_color(&frame, &mut hsv, imgproc::COLOR_BGR2HSV, 0)?;

        // Red color has two ranges in HSV
        let lower_red1 = core::Scalar::new(0.0, 120.0, 70.0, 0.0);
        let upper_red1 = core::Scalar::new(10.0, 255.0, 255.0, 0.0);

        let lower_red2 = core::Scalar::new(170.0, 120.0, 70.0, 0.0);
        let upper_red2 = core::Scalar::new(180.0, 255.0, 255.0, 0.0);

        let mut mask1 = Mat::default();
        core::in_range(&hsv, &lower_red1, &upper_red1, &mut mask1)?;

        let mut mask2 = Mat::default();
        core::in_range(&hsv, &lower_red2, &upper_red2, &mut mask2)?;

        let mut mask = Mat::default();
        core::bitwise_or(&mask1, &mask2, &mut mask, &core::no_array()?)?;

        // Create highlighted image
        let mut highlighted = frame.clone();
        let red_overlay = core::Scalar::new(0.0, 0.0, 255.0, 0.0);
        highlighted.set_to(&red_overlay, &mask)?;

        // Blend with original frame for highlighting effect
        let mut output = Mat::default();
        core::add_weighted(&frame, 0.7, &highlighted, 0.3, 0.0, &mut output, -1)?;

        highgui::imshow("Red Object Highlight", &output)?;
        if highgui::wait_key(1)? == 113 {
            // 'q'
            break;
        }
    }

    Ok(())
}
