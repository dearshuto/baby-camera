use std::{
    path::Path,
    process::Stdio,
    sync::{Arc, Mutex},
};

use warp::{Filter, reject::Rejection, reply::Reply};

pub struct Server {}

impl Server {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn serve<T>(self, html_path: T, port: u16)
    where
        T: AsRef<Path>,
    {
        // ルートパス("/")へのアクセスに対して、指定されたパスの html を返す
        // TODO: エラーチェック
        let route = warp::path::end().and(warp::fs::file(html_path.as_ref().to_path_buf()));

        // とりあえずカレントをリソース置き場へ
        let res_files = warp::path("static").and(warp::fs::dir("."));

        let process = Arc::new(Mutex::new(None));
        let process = warp::get()
            .and(warp::path("libcamera-vid"))
            .and(warp::any().map(move || process.clone()))
            .and_then(Self::spawn_process);

        let filter = route.or(process).or(res_files);

        // サーバーを 0.0.0.0:8080 で起動
        let socket_addr =
            std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), port);
        warp::serve(filter).run(socket_addr).await;
    }

    async fn spawn_process(
        process: Arc<Mutex<Option<std::process::Child>>>,
    ) -> Result<impl Reply, Rejection> {
        let mut binding = process.lock().unwrap();

        if let Some(mut process) = binding.take() {
            process.kill().unwrap();
        }

        *binding = Some(
            std::process::Command::new("libcamera-vid")
                .args([
                    "-t",
                    "0",
                    "--framerate",
                    "20",
                    "--inline",
                    "-o",
                    "-",
                    "--width",
                    "640",
                    "--height",
                    "480",
                    "--codec",
                    "mjpeg",
                ])
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );

        Ok(warp::reply::with_status(
            format!("Process started with PID: {}", 10),
            warp::http::StatusCode::OK,
        ))
        //  Ok(warp::reply::reply())
    }
}
