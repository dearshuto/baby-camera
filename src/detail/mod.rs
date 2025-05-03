mod generic_stream;
mod polling_task;
mod stdin_stream;

pub use generic_stream::GenericStream;
pub use polling_task::PollingTask;
pub use stdin_stream::ReadStream;
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
