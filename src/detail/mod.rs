mod generic_stream;

pub use generic_stream::GenericStream;

pub trait VideoStream {
    type Buffer;

    fn new_buffer() -> Self::Buffer;

    fn read(&mut self, buffer: &mut Self::Buffer) -> usize;
}
