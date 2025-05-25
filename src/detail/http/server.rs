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

        // index.html の横に admin.html が置いてあることを期待
        let admin_html_path = {
            let mut html_path = html_path.as_ref().parent().unwrap().to_path_buf();
            html_path.push("admin.html");
            html_path
        };
        let admin = warp::path("admin").and(warp::fs::file(admin_html_path));

        // とりあえずカレントをリソース置き場へ
        let res_files = warp::path("assets").and(warp::fs::dir("assets"));
        let res_local = warp::fs::dir(".");

        let process = Arc::new(Mutex::new(None));
        let process = warp::get()
            .and(warp::path("libcamera-vid"))
            .and(warp::any().map(move || process.clone()))
            .and_then(Self::spawn_process);

        let filter = route.or(admin).or(process).or(res_files).or(res_local);

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
