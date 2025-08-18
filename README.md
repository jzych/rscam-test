# rscam-test

## Setup

### Enable camera overlay (for PiOS 12)

Run: `sudo nano /boot/firmware/config.txt`

Add this line at the bottom: `dtoverlay=ov5647`

Save and exit, then reboot: `sudo reboot`

Verify camera works: `libcamera-hello`

### Install GStreamer

Run:

`sudo apt install -y \
    libgstreamer1.0-dev \
    gstreamer1.0-tools \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav \
    gstreamer1.0-gl \
    gstreamer1.0-gtk3 \
    gstreamer1.0-apps`

Verify:

`gst-launch-1.0 libcamerasrc ! videoconvert ! autovideosink`

### Install OpenCV

Run:

`sudo apt install -y libopencv-dev`

Note: to build OpenCV you need more then 1GB od RAM, best more then 4GB. In case of shortage increase you swap partition to at least 2GB.