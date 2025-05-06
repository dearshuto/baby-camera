mod detail;

use clap::{Parser, Subcommand};
use detail::{CaptureTask, GenericStream, PollingTask, ReadStream, VideoStream};

use std::net::Ipv4Addr;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use tokio::sync::RwLock;

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

        /// 子プロセスとして実行する外部コマンドを指定します
        /// 別途用意されたキャプチャーサーバーを子プロセスとして紐づけるケースを想定しています
        #[arg(long)]
        external_command: Option<String>,
    },

    /// REST API を使用して対話的なサーバーを起動します
    Http {
        #[arg(short, long, default_value_t = 8080)]
        port: u16,

        #[arg(long, default_value_t = String::from("index.html"))]
        html: String,
    },
}

struct StreamData<T> {
    image_data: String,
    buffer_data: Arc<RwLock<T>>,
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
                PollingTask::new(Duration::from_millis(tick))
                    .run(video_stream, observer_receiver, sender)
                    .await
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
            external_command,
        } => {
            let tcp_stream = if let Some(external_command) = external_command {
                // 外部コマンドあり
                let command = std::process::Command::new(external_command);
                detail::TcpStream::new_with_process(listen_socket_addr, command)
            } else {
                // 素の TCP ストリームを起動
                detail::TcpStream::new(listen_socket_addr)
            };
            main_impl(tcp_stream, port, tick).await;
        }
        StreamType::Http { port, html } => {
            detail::HttpServer::new().serve(html, port).await;
        }
    }
}
