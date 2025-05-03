use std::{net::ToSocketAddrs, process::Command, time::Duration};

use super::{ReadStream, VideoStream};

pub struct TcpStream {
    socket_addr: std::net::SocketAddr,
    internal_stream: Option<ReadStream<std::net::TcpStream>>,
    command: Option<Command>,
    child_process: Option<std::process::Child>,
}

impl TcpStream {
    pub fn new<T>(addr: T) -> Self
    where
        T: ToSocketAddrs,
    {
        Self {
            socket_addr: addr.to_socket_addrs().unwrap().next().unwrap(),
            internal_stream: None,
            command: None,
            child_process: None,
        }
    }

    pub fn new_with_process<T>(addr: T, command: Command) -> Self
    where
        T: ToSocketAddrs,
    {
        Self {
            socket_addr: addr.to_socket_addrs().unwrap().next().unwrap(),
            internal_stream: None,
            command: Some(command),
            child_process: None,
        }
    }
}

impl VideoStream for TcpStream {
    type Buffer = <ReadStream<std::net::TcpStream> as VideoStream>::Buffer;

    fn new_buffer() -> Self::Buffer {
        <ReadStream<std::net::TcpStream> as VideoStream>::new_buffer()
    }

    fn read(&mut self, buffer: &mut Self::Buffer) -> usize {
        // 外部コマンドの起動
        if let Some(command) = &mut self.command {
            if let Some(process) = &mut self.child_process {
                match process.try_wait() {
                    Ok(Some(_)) => {
                        // 外部プロセスが終了していた
                        // MEMO: 再起動すべき？
                    }
                    Ok(None) => {
                        // プロセスは実行中なのでなにもしない
                    }
                    Err(_error) => {
                        // プロセスの状態が取得できない場合だがどういう状態？
                        todo!()
                    }
                }
            } else {
                // コマンドは指定されているがまだ起動していなかったのでプロセスを作成する
                self.child_process = Some(command.spawn().expect("faild to spawn"));
            }
        }

        // TCP 接続
        let stream = self.internal_stream.get_or_insert_with(|| {
            // MEMO: とりあえず 1 分でタイムアウトにしているが、外部から指定できたほうがいいと思う
            let duration = Duration::from_secs(60);
            let reader = std::net::TcpStream::connect_timeout(&self.socket_addr, duration).unwrap();
            ReadStream::new(reader).unwrap()
        });
        stream.read(buffer)
    }
}
