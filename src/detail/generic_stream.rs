use std::ops::Deref;

use chrono::Timelike;
use opencv::{
    core::{Mat, Point2i, VecN, Vector},
    imgcodecs,
    videoio::{CAP_ANY, VideoCapture, VideoCaptureTrait},
};

use super::VideoStream;

pub struct GenericStreamBuffer(Vector<u8>);

impl Deref for GenericStreamBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

pub struct GenericStream {
    video_capture: VideoCapture,
    frame: Mat,
}

impl GenericStream {
    pub fn new(camera_index: i32) -> Result<Self, ()> {
        let Ok(video_capture) = VideoCapture::new(camera_index, CAP_ANY) else {
            return Err(());
        };
        let frame = Mat::default();

        Ok(Self {
            video_capture,
            frame,
        })
    }

    fn read(&mut self, buffer: &mut GenericStreamBuffer) -> usize {
        let Ok(_) = self.video_capture.read(&mut self.frame) else {
            return 0;
        };

        buffer.0.clear();

        let current_time = chrono::Local::now();
        if let Err(_) = opencv::imgproc::put_text(
            &mut self.frame,
            &format!(
                "{}:{} {}",
                current_time.hour(),
                current_time.minute(),
                current_time.second()
            ),
            Point2i::new(50, 50),
            opencv::imgproc::FONT_HERSHEY_SIMPLEX,
            1.5,
            VecN::new(1.0, 255.0, 1.0, 255.0),
            1,
            opencv::imgproc::LINE_4,
            false,
        ) {
            // もみ消す
        }

        let _ = imgcodecs::imencode(".jpg", &self.frame, &mut buffer.0, &Vector::new());

        buffer.len()
    }
}

impl VideoStream for GenericStream {
    type Buffer = GenericStreamBuffer;

    fn new_buffer() -> Self::Buffer {
        GenericStreamBuffer(Vector::new())
    }

    fn read(&mut self, buffer: &mut Self::Buffer) -> usize {
        self.read(buffer)
    }
}
