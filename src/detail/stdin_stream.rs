use std::{
    io::{BufReader, Read},
    ops::Deref,
};

use super::VideoStream;

pub struct ReadBuffer {
    start: usize,
    end: usize,
    buffer: Vec<u8>,
}

impl ReadBuffer {
    pub fn shrink(&mut self) {
        if self.end == 0 {
            return;
        }
        self.buffer = self.buffer.drain((self.end + 1)..).collect();
        self.start = 0;
        self.end = 0;
    }
}

impl Deref for ReadBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.buffer[self.start..=self.end]
    }
}

pub struct ReadStream<T> {
    reader: BufReader<T>,
}

impl<T> ReadStream<T>
where
    T: std::io::Read,
{
    pub fn new(read: T) -> Result<Self, ()> {
        let reader = BufReader::new(read);

        Ok(Self { reader })
    }
}

impl<T> VideoStream for ReadStream<T>
where
    T: std::io::Read,
{
    type Buffer = ReadBuffer;

    fn new_buffer() -> Self::Buffer {
        ReadBuffer {
            start: 0,
            end: 0,
            buffer: Vec::default(),
        }
    }

    fn read(&mut self, buffer: &mut Self::Buffer) -> usize {
        buffer.shrink();

        // 完全な jpg データが届くまで入力を受け付ける
        let (start, end) = loop {
            // バイト列を読み込む
            let mut chunk = [0; 1024];
            let size = self.reader.read(&mut chunk).unwrap();

            // 既存のバッファーに追加
            buffer.buffer.extend_from_slice(&chunk[..size]);

            let Some(range) = self::find_jpg(&buffer.buffer) else {
                continue;
            };

            break range;
        };

        buffer.start = start;
        buffer.end = end;

        (end - start) + 1
    }
}

fn find_jpg(data: &[u8]) -> Option<(usize, usize)> {
    // jpg 画像が完成したかチェック
    for index in 0..(data.len() - 1) {
        // jpg 開始のマーカーを探す
        if !(data[index] == 0xFF && data[index + 1] == 0xD8) {
            continue;
        }

        for j in (index + 2)..(data.len() - 1) {
            if !(data[j] == 0xFF && data[j + 1] == 0xD9) {
                continue;
            }

            // EOI マーカーが見つかった
            return Some((index, j + 1));
        }

        return None;
    }

    // jog 画像が完成してなかった
    None
}
