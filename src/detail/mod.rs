#[cfg(feature = "opencv")]
mod generic_stream;

#[cfg(not(feature = "opencv"))]
mod generic_stream {
    use super::VideoStream;

    pub struct GenericStream;

    impl GenericStream {
        pub fn new(_: i32) -> Result<Self, ()> {
            Ok(Self {})
        }
    }

    impl VideoStream for GenericStream {
        type Buffer = Vec<u8>;

        fn new_buffer() -> Self::Buffer {
            Vec::default()
        }

        fn read(&mut self, _buffer: &mut Self::Buffer) -> usize {
            0
        }
    }
}

mod polling_task;
mod stdin_stream;
mod tcp_stream;

pub use generic_stream::GenericStream;
pub use polling_task::PollingTask;
pub use stdin_stream::ReadStream;
pub use tcp_stream::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::StreamData;

pub trait VideoStream {
    type Buffer;

    fn new_buffer() -> Self::Buffer;

    fn read(&mut self, buffer: &mut Self::Buffer) -> usize;
}

pub trait CaptureTask {
    async fn run<T>(
        &mut self,
        video_stream: T,
        sender_receiver: Receiver<Sender<StreamData<T::Buffer>>>,
        init_receiver: Sender<StreamData<T::Buffer>>,
    ) -> (T, Receiver<Sender<StreamData<T::Buffer>>>)
    where
        T: VideoStream;
}
