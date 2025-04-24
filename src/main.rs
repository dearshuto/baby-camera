mod detail;

use clap::{Parser, Subcommand};
use detail::{GenericStream, ReadStream, VideoStream};

use std::net::Ipv4Addr;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use tokio::sync::RwLock;
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    subcommand: StreamType,
}

#[derive(Debug, Subcommand)]
enum StreamType {
    /// 接続されているカメラデバイスからキャプチャーします
    /// 利用できるデバイスはインストールされているドライバーに依存します
    Device {
        /// millisec
        #[arg(long, default_value_t = 200)]
        tick: u64,

        #[arg(short, long, default_value_t = 8080)]
        port: u16,

        #[arg(long, default_value_t = 0)]
        camera: i32,
    },

    /// 標準入力に渡されたデータをキャプチャーデータとして使用します
    Stdin {
        /// millisec
        #[arg(long, default_value_t = 200)]
        tick: u64,

        #[arg(short, long, default_value_t = 8080)]
        port: u16,
    },

    /// TCP 通信でキャプチャーデータを取得します
    Tcp {
        /// millisec
        #[arg(long, default_value_t = 200)]
        tick: u64,

        #[arg(short, long, default_value_t = 8080)]
        port: u16,

        #[arg(long, default_value_t = String::from("localhost:8081"))]
        listen_socket_addr: String,
    },
}

struct StreamData<T> {
    image_data: String,
    buffer_data: Arc<RwLock<T>>,
}

async fn serve_camera_impl<TVideoStream>(
    mut video_stream: TVideoStream,
    mut sender_receiver: Receiver<Sender<StreamData<TVideoStream::Buffer>>>,
    init_receiver: Sender<StreamData<TVideoStream::Buffer>>,
    tick: u64, // millisec
) -> (
    TVideoStream,
    Receiver<Sender<StreamData<TVideoStream::Buffer>>>,
)
where
    TVideoStream: VideoStream,
    <TVideoStream as VideoStream>::Buffer: Sync,
{
    let mut senders = vec![init_receiver];
    let buf = Arc::new(RwLock::new(TVideoStream::new_buffer()));

    loop {
        // サンプリング間隔の調整
        // 細かくすると動画がなめらかになるが CPU 負荷が高くなる
        // TODO: 外部からパラメーターで指定できるようにしたい
        tokio::time::sleep(Duration::from_millis(tick)).await;

        if senders.is_empty() {
            // 観測者が誰もいなかったので終了
            return (video_stream, sender_receiver);
        } else {
            // すでに観測者がいる場合は追加の観測者がいないか確認する
            if let Ok(sender) = sender_receiver.try_recv() {
                senders.push(sender);
            }
        }

        let image_data = {
            let mut buf = buf.write().await;
            let length = video_stream.read(&mut buf);

            format!(
                "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                length
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

async fn serve<T>(mut stream: TcpStream, mut receiver: tokio::sync::mpsc::Receiver<StreamData<T>>)
where
    T: Deref<Target = [u8]>,
{
    while let Some(data) = receiver.recv().await {
        let Ok(_) = stream.write_all(data.image_data.as_bytes()).await else {
            break;
        };

        let buffer = data.buffer_data.read().await;
        let Ok(_) = stream.write_all(&buffer).await else {
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

async fn main_impl<T>(video_stream: T, port: u16, tick: u64)
where
    T: VideoStream + std::marker::Send + 'static,
    <T as VideoStream>::Buffer: Sync,
    <T as VideoStream>::Buffer: Send,
    <T as VideoStream>::Buffer: Deref<Target = [u8]>,
    <T as VideoStream>::Buffer: 'static,
{
    // 通信を開始できなければ終了
    let socket_addr = std::net::SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    let listener = TcpListener::bind(socket_addr).await.unwrap();

    let (observer_sender, observer_receiver) = tokio::sync::mpsc::channel(1);

    // カメラ映像のキャプチャータスクの初期値
    // カメラとレシーバーの所有権を移譲するために両インスタンスを即時返すタスクを発行している
    let mut handle = tokio::spawn(async move { (video_stream, observer_receiver) });

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
            let (video_stream, observer_receiver) = handle.await.unwrap();
            handle = tokio::spawn(async move {
                let tick = tick.clamp(20, 1000);
                serve_camera_impl(video_stream, observer_receiver, sender, tick).await
            });
        } else {
            observer_sender.send(sender).await.unwrap_or_default();
        }
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    match args.subcommand {
        StreamType::Device { tick, port, camera } => {
            let Ok(video_stream) = GenericStream::new(camera) else {
                // カメラの設定に失敗したら終了
                panic!()
            };

            main_impl(video_stream, port, tick).await;
        }
        StreamType::Stdin { tick, port } => {
            let reader = std::io::stdin();
            let read_stream = ReadStream::new(reader).unwrap();
            main_impl(read_stream, port, tick).await;
        }
        StreamType::Tcp {
            tick,
            port,
            listen_socket_addr,
        } => {
            let reader = std::net::TcpStream::connect(listen_socket_addr).unwrap();
            let read_stream = ReadStream::new(reader).unwrap();
            main_impl(read_stream, port, tick).await;
        }
    }
}
