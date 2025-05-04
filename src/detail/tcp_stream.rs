use std::{net::ToSocketAddrs, time::Duration};

use super::{ReadStream, VideoStream};

pub struct TcpStream {
    socket_addr: std::net::SocketAddr,
    internal_stream: Option<ReadStream<std::net::TcpStream>>,
}

impl TcpStream {
    pub fn new<T>(addr: T) -> Self
    where
        T: ToSocketAddrs,
    {
        Self {
            socket_addr: addr.to_socket_addrs().unwrap().next().unwrap(),
            internal_stream: None,
        }
    }
}

impl VideoStream for TcpStream {
    type Buffer = <ReadStream<std::net::TcpStream> as VideoStream>::Buffer;

    fn new_buffer() -> Self::Buffer {
        <ReadStream<std::net::TcpStream> as VideoStream>::new_buffer()
    }

    fn read(&mut self, buffer: &mut Self::Buffer) -> usize {
        let stream = self.internal_stream.get_or_insert_with(|| {
            // MEMO: とりあえず 1 分でタイムアウトにしているが、外部から指定できたほうがいいと思う
            let duration = Duration::from_secs(60);
            let reader = std::net::TcpStream::connect_timeout(&self.socket_addr, duration).unwrap();
            ReadStream::new(reader).unwrap()
        });
        stream.read(buffer)
    }
}
