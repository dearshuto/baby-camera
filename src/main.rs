use std::io::Write;
use std::net::TcpListener;

use opencv::core::Vector;
use opencv::{Result, videoio};
use opencv::{imgcodecs, prelude::*};

fn main() -> Result<()> {
    let listener = TcpListener::bind("192.168.1.100:8080").unwrap();

    let mut cam = videoio::VideoCapture::new(0, videoio::CAP_ANY)?; // 0 is the default camera
    let mut frame = Mat::default();
    let opened = videoio::VideoCapture::is_opened(&cam)?;
    if !opened {
        panic!("Unable to open default camera!");
    }

    let mut buf = opencv::core::Vector::new();
    loop {
        let (mut stream, _) = listener.accept().expect("Failed to accept connection");
        cam.read(&mut frame).expect("Failed to capture frame");
        buf.clear();
        let _ = imgcodecs::imencode(".jpg", &frame, &mut buf, &Vector::new());

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\n\r\n"
        );

        stream.write_all(response.as_bytes()).unwrap();

        loop {
            cam.read(&mut frame)?;
            buf.clear();
            let _ = imgcodecs::imencode(".jpg", &frame, &mut buf, &Vector::new());

            let image_data = format!(
                "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                buf.len()
            );
            let Ok(_) = stream.write_all(image_data.as_bytes()) else {
                break;
            };

            stream.write_all(buf.as_slice()).unwrap();
            stream.write_all(b"\r\n").unwrap();
            stream.flush().unwrap();
        }
    }
}
