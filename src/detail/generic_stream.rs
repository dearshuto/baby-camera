use chrono::Timelike;
use opencv::{
    core::{Mat, Point2i, VecN, Vector},
    imgcodecs,
    videoio::{CAP_ANY, VideoCapture, VideoCaptureTrait},
};

use super::VideoStream;

pub struct GenericStream {
    video_capture: VideoCapture,
    frame: Mat,
}

impl GenericStream {
    pub fn new() -> Result<Self, ()> {
        let Ok(video_capture) = VideoCapture::new(0, CAP_ANY) else {
            return Err(());
        };
        let frame = Mat::default();

        Ok(Self {
            video_capture,
            frame,
        })
    }

    fn read(&mut self, buffer: &mut opencv::core::Vector<u8>) -> usize {
        let Ok(_) = self.video_capture.read(&mut self.frame) else {
            return 0;
        };

        buffer.clear();

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

        let _ = imgcodecs::imencode(".jpg", &self.frame, buffer, &Vector::new());

        buffer.len()
    }
}

impl VideoStream for GenericStream {
    type Buffer = Vector<u8>;

    fn new_buffer() -> Self::Buffer {
        Vector::new()
    }

    fn read(&mut self, buffer: &mut Self::Buffer) -> usize {
        self.read(buffer)
    }
}
