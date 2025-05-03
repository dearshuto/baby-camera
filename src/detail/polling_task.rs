use std::{sync::Arc, time::Duration};

use tokio::sync::RwLock;

use crate::StreamData;

use super::{CaptureTask, VideoStream};

pub struct PollingTask {
    tick: Duration,
}

impl PollingTask {
    pub fn new(tick: Duration) -> Self {
        Self { tick }
    }
}

impl CaptureTask for PollingTask {
    async fn run<T>(
        &mut self,
        mut video_stream: T,
        mut sender_receiver: tokio::sync::mpsc::Receiver<
            tokio::sync::mpsc::Sender<StreamData<T::Buffer>>,
        >,
        init_receiver: tokio::sync::mpsc::Sender<StreamData<T::Buffer>>,
    ) -> (
        T,
        tokio::sync::mpsc::Receiver<tokio::sync::mpsc::Sender<StreamData<T::Buffer>>>,
    )
    where
        T: VideoStream,
    {
        let mut senders = vec![init_receiver];
        let buf = Arc::new(RwLock::new(T::new_buffer()));

        loop {
            // サンプリング間隔の調整
            // 細かくすると動画がなめらかになるが CPU 負荷が高くなる
            // TODO: 外部からパラメーターで指定できるようにしたい
            tokio::time::sleep(self.tick).await;

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
}
