use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use chrono::Timelike;
use clap::Parser;
use opencv::videoio::VideoCapture;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use opencv::core::{Point2i, VecN, Vector};
use opencv::{Result, videoio};
use opencv::{imgcodecs, prelude::*};
use tokio::sync::RwLock;
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// millisec
    #[arg(long, default_value_t = 200)]
    tick: u64,
}

struct StreamData {
    image_data: String,
    buffer_data: Arc<RwLock<opencv::core::Vector<u8>>>,
}

async fn serve_camera(
    mut camera: VideoCapture,
    mut sender_receiver: Receiver<Sender<StreamData>>,
    init_receiver: Sender<StreamData>,
    tick: u64, // millisec
) -> (VideoCapture, Receiver<Sender<StreamData>>) {
    let mut senders = vec![init_receiver];
    let mut frame = Mat::default();
    let buf = Arc::new(RwLock::new(opencv::core::Vector::new()));

    loop {
        // サンプリング間隔の調整
        // 細かくすると動画がなめらかになるが CPU 負荷が高くなる
        // TODO: 外部からパラメーターで指定できるようにしたい
        tokio::time::sleep(Duration::from_millis(tick)).await;

        if senders.is_empty() {
            // 観測者が誰もいなかったので終了
            return (camera, sender_receiver);
        } else {
            // すでに観測者がいる場合は追加の観測者がいないか確認する
            if let Ok(sender) = sender_receiver.try_recv() {
                senders.push(sender);
            }
        }

        let image_data = {
            let mut buf = buf.write().await;
            camera.read(&mut frame).expect("Failed to capture frame");
            buf.clear();

            let current_time = chrono::Local::now();
            if let Err(_) = opencv::imgproc::put_text(
                &mut frame,
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

            let _ = imgcodecs::imencode(".jpg", &frame, &mut buf, &Vector::new());

            format!(
                "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                buf.len()
            )
        };

        // ストリーム用のデータを通知
        let is_shrink_needed = {
            let mut handles = Vec::default();
            for sender in &senders {
                // 送信処理はすべて並列実行
                let stream_data = StreamData {
                    image_data: image_data.clone(),
                    buffer_data: buf.clone(),
                };

                handles.push(sender.send(stream_data));
            }

            // 送信処理の完了待ち
            // MEMO: 遅いストリームがあるとそれが律速となるが気にしなくてよい？
            let results = futures_util::future::join_all(handles).await;

            // 送信に失敗したストリームがあればフラグを立てて後段でキューを更新する
            results.iter().any(|x| x.is_err())
        };

        // 閉じているチャンネルがあれば送信キューから削除
        if is_shrink_needed {
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
        drop(buffer);

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
    let args = Args::parse();

    // 0 is the default camera
    let camera = videoio::VideoCapture::new(0, videoio::CAP_ANY)?;

    // カメラの設定に失敗したら終了
    let opened = videoio::VideoCapture::is_opened(&camera)?;
    if !opened {
        panic!("Unable to open default camera!");
    }

    // 通信を開始できなければ終了
    let socket_addr = std::net::SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, args.port);
    let listener = TcpListener::bind(socket_addr).await.unwrap();

    let (observer_sender, observer_receiver) = tokio::sync::mpsc::channel(1);

    // カメラ映像のキャプチャータスクの初期値
    // カメラとレシーバーの所有権を移譲するために両インスタンスを即時返すタスクを発行している
    let mut handle = tokio::spawn(async move { (camera, observer_receiver) });

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

        // 観測者が誰もいなくなってタスクが終了している
        // この場合は再起動する
        if handle.is_finished() {
            let (camera, observer_receiver) = handle.await.unwrap();
            handle = tokio::spawn(async move {
                let tick = args.tick.clamp(20, 1000);
                serve_camera(camera, observer_receiver, sender, tick).await
            });
        } else {
            observer_sender.send(sender).await.unwrap_or_default();
        }
    }
}
