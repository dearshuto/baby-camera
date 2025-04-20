use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use opencv::videoio::VideoCapture;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use opencv::core::Vector;
use opencv::{Result, videoio};
use opencv::{imgcodecs, prelude::*};
use tokio::sync::RwLock;
use tokio::sync::mpsc::Sender;

struct StreamData {
    image_data: String,
    buffer_data: Arc<RwLock<opencv::core::Vector<u8>>>,
}

async fn serve_camera(mut camera: VideoCapture, senders: Arc<RwLock<Vec<Sender<StreamData>>>>) {
    let mut frame = Mat::default();
    let buf = Arc::new(RwLock::new(opencv::core::Vector::new()));
    loop {
        // 観測者が誰もいなければ何もしないで処理をスカす
        {
            let senders = senders.read().await;
            if senders.is_empty() {
                tokio::time::sleep(Duration::from_millis(1000)).await;
                continue;
            }
        }

        let image_data = {
            let mut buf = buf.write().await;
            camera.read(&mut frame).expect("Failed to capture frame");
            buf.clear();
            let _ = imgcodecs::imencode(".jpg", &frame, &mut buf, &Vector::new());

            format!(
                "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                buf.len()
            )
        };

        // ストリーム用のデータを通知
        // ここは読み取りロック
        //
        // 通知処理と後片付けを同時にやろうとすると書き込みロックが必要なので、
        // 読み取りロックだけで事足りるように通知処理と分けている
        let is_shrink_needed = {
            let mut is_shrink_needed = false;
            let senders = senders.read().await;
            for sender in senders.iter() {
                let stream_data = StreamData {
                    image_data: image_data.clone(),
                    buffer_data: buf.clone(),
                };

                if let Err(_) = sender.send(stream_data).await {
                    is_shrink_needed = true;
                }
            }
            is_shrink_needed
        };

        // 閉じているチャンネルがあれば送信キューから削除
        // ここは書き込みロック
        if is_shrink_needed {
            let mut senders = senders.write().await;
            senders.retain(|sender| !sender.is_closed());
        }
    }
}

async fn serve(mut stream: TcpStream, mut receiver: tokio::sync::mpsc::Receiver<StreamData>) {
    while let Some(data) = receiver.recv().await {
        let Ok(_) = stream.write_all(data.image_data.as_bytes()).await else {
            break;
        };

        let buffer = data.buffer_data.read().await;
        let Ok(_) = stream.write_all(&buffer.as_slice()).await else {
            break;
        };

        let Ok(_) = stream.write_all(b"\r\n").await else {
            break;
        };

        let Ok(_) = stream.flush().await else {
            break;
        };
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 0 is the default camera
    let camera = videoio::VideoCapture::new(0, videoio::CAP_ANY)?;

    // カメラの設定に失敗したら終了
    let opened = videoio::VideoCapture::is_opened(&camera)?;
    if !opened {
        panic!("Unable to open default camera!");
    }

    // 通信を開始できなければ終了
    let socket_addr = std::net::SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8080);
    let listener = TcpListener::bind(socket_addr).await.unwrap();

    let senders = Arc::default();

    // カメラ映像のキャプチャータスク
    let senders_local = Arc::clone(&senders);
    tokio::spawn(async move {
        serve_camera(camera, senders_local).await;
    });

    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("Failed to accept connection");

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\n\r\n"
        );

        stream.write_all(response.as_bytes()).await.unwrap();

        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            serve(stream, receiver).await;
        });

        senders.write().await.push(sender);
    }
}
