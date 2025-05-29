use std::{process::Stdio, sync::Arc};

use serde::Deserialize;
use tokio::sync::Mutex;
use warp::{Filter, reject::Rejection, reply::Reply};

#[derive(Debug, Clone, Deserialize)]
pub struct ProcessPipe {
    process_list: Vec<ProcessData>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProcessData {
    pub name: String,
    pub args: Vec<String>,
}

pub struct ExternalProcess;

impl ExternalProcess {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn serve(self, process_pipe: ProcessPipe, port: u16) {
        let process = Arc::new(Mutex::new(None));

        let process_local = process.clone();
        let restart = warp::get()
            .and(warp::path("restart"))
            .and(warp::any().map(move || process_local.clone()))
            .and(warp::any().map(move || process_pipe.clone()))
            .and_then(Self::spawn_process);

        let process_local = process.clone();
        let kill = warp::get()
            .and(warp::path("kill"))
            .and(warp::any().map(move || process_local.clone()))
            .and_then(Self::kill_process);

        let filter = restart.or(kill);

        // サーバーを起動
        let socket_addr =
            std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), port);
        warp::serve(filter).run(socket_addr).await;
    }

    async fn spawn_process(
        process_list: Arc<Mutex<Option<Vec<std::process::Child>>>>,
        process_pipe: ProcessPipe,
    ) -> Result<impl Reply, Rejection> {
        let mut binding = process_list.lock().await;

        if let Some(process_list) = binding.take() {
            for mut process in process_list {
                process.kill().unwrap();
            }
        }

        let last_index = process_pipe.process_list.len() - 1;
        let mut process_list = Vec::default();
        let mut stdin_previous = None;
        for (index, process) in process_pipe.process_list.into_iter().enumerate() {
            // 直前の標準出力があればパイプする
            let stdin = if let Some(stdin) = stdin_previous.take() {
                Stdio::from(stdin)
            } else {
                Stdio::null()
            };

            // 最後のコマンド以外は標準出力をパイプする
            let stdout = if index == last_index {
                Stdio::null()
            } else {
                Stdio::piped()
            };

            let mut process = std::process::Command::new(process.name)
                .args(process.args)
                .stdin(stdin)
                .stdout(stdout)
                .spawn()
                .unwrap();

            // 標準出力は次のコマンドのために取っておく
            stdin_previous = process.stdout.take();
            process_list.push(process);
        }

        let id_list: Vec<_> = process_list.iter().map(|x| x.id()).collect();

        *binding = Some(process_list);

        Ok(warp::reply::with_status(
            format!("Process started with PID: {:?}", id_list),
            warp::http::StatusCode::OK,
        ))
        //  Ok(warp::reply::reply())
    }

    async fn kill_process(
        process_list: Arc<Mutex<Option<Vec<std::process::Child>>>>,
    ) -> Result<impl Reply, Rejection> {
        let mut binding = process_list.lock().await;
        let Some(children) = binding.take() else {
            return Err(warp::reject());
        };

        let mut id_list = Vec::default();
        for mut child in children {
            id_list.push(child.id());
            child.kill().unwrap();
        }

        Ok(warp::reply::with_status(
            format!("Process kill with PID: {:?}", id_list),
            warp::http::StatusCode::OK,
        ))
    }
}
