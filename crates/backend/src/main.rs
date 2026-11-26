//! homeitem backend entry point.

mod api;
mod assets;
mod auth;
mod config;
mod db;
mod error;
mod mail;
mod reminders;
mod router;

use crate::config::Config;
use salvo::conn::Listener;

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async_main());
}

async fn async_main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env();
    tracing::info!(version = common::APP_VERSION, addr = %config.listen_addr, "homeitem starting");

    let pool = match db::init(&config) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "database init failed");
            std::process::exit(1);
        }
    };
    if let Err(e) = db::init_upload_dir(&config) {
        tracing::error!(error = %e, "upload dir init failed");
        std::process::exit(1);
    }

    // background reminder scanner (email push)
    reminders::spawn(pool.clone(), config.clone());

    let app = router::router(pool, &config);
    let cors = router::build_cors(&config);

    let service = salvo::Service::new(app).hoop(cors.into_handler());
    let acceptor = salvo::conn::TcpListener::new(config.listen_addr.clone()).bind().await;
    tracing::info!(addr = %config.listen_addr, "listening");
    salvo::Server::new(acceptor).serve(service).await;
}
