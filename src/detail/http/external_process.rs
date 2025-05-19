use std::{process::Stdio, sync::Arc};

use serde::Deserialize;
use tokio::sync::Mutex;
use warp::{Filter, reject::Rejection, reply::Reply};

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

    pub async fn serve(self, process_data: ProcessData, port: u16) {
        let process = Arc::new(Mutex::new(None));

        let process_local = process.clone();
        let restart = warp::get()
            .and(warp::path("restart"))
            .and(warp::any().map(move || process_local.clone()))
            .and(warp::any().map(move || process_data.clone()))
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
        process: Arc<Mutex<Option<std::process::Child>>>,
        process_data: ProcessData,
    ) -> Result<impl Reply, Rejection> {
        let mut binding = process.lock().await;

        if let Some(mut process) = binding.take() {
            process.kill().unwrap();
        }

        let process = std::process::Command::new(process_data.name)
            .args(process_data.args)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let id = process.id();

        *binding = Some(process);

        Ok(warp::reply::with_status(
            format!("Process started with PID: {}", id),
            warp::http::StatusCode::OK,
        ))
        //  Ok(warp::reply::reply())
    }

    async fn kill_process(
        process: Arc<Mutex<Option<std::process::Child>>>,
    ) -> Result<impl Reply, Rejection> {
        let mut binding = process.lock().await;
        let Some(mut child) = binding.take() else {
            return Err(warp::reject());
        };

        let id = child.id();
        child.kill().unwrap();

        Ok(warp::reply::with_status(
            format!("Process kill with PID: {}", id),
            warp::http::StatusCode::OK,
        ))
    }
}
