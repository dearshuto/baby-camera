use clap::Parser;
use warp::Filter;

#[derive(Debug, clap::Parser)]
struct Args {
    #[arg(short, long, required = false)]
    media: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let media_filter = warp::path("media").and(warp::fs::dir(std::path::PathBuf::from(
        args.media.unwrap_or_default(),
    )));

    // ルートパス("/")へのアクセスに対して、Hello, Warp! というHTMLを返す
    let route = warp::path::end().map(|| warp::reply::html(include_str!("index.html")));

    let filter = route.or(media_filter);

    // サーバーを 0.0.0.0:8080 で起動
    warp::serve(filter).run(([0, 0, 0, 0], 8080)).await;
}
