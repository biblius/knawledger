use clap::Parser;
use std::{collections::HashSet, num::NonZeroUsize};
use tracing::{info, Level};

use crate::{config::Config, db::Database, document::process_directory, state::State};

pub const FILES_PER_THREAD: usize = 128;

lazy_static::lazy_static! {
    pub static ref MAX_THREADS: usize = std::thread::available_parallelism().unwrap_or(NonZeroUsize::new(1).unwrap()).into();
}

pub mod chunk;
pub mod config;
pub mod db;
pub mod document;
pub mod error;
pub mod htmx;
pub mod notifiy;
pub mod router;
pub mod state;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");

    let host = "127.0.0.1";
    let port = "3002";

    let addr = format!("{host}:{port}");

    let config = Config::parse();

    let database = Database::new(&db_url).await;

    // Trim any directories that should not be loaded
    database
        .trim_unused(&config.directories)
        .await
        .expect("could not trim directories");

    for dir in config.directories.iter() {
        process_directory(&database, dir, None)
            .await
            .expect("unable to process directory");
    }

    // let (tx, rx) = std::sync::mpsc::channel();

    // let roots = database
    //     .list_root_paths()
    //     .await
    //     .expect("unable to process roots")
    //     .into_iter()
    //     .collect::<HashSet<_>>();

    // let notifier = NotifyHandler::new(database.clone(), roots, rx);

    // let handle = notifier.run().expect("could not start watcher");

    // let handle = NotifierHandle { tx, handle };

    let state = State::new(database.clone(), config);

    info!("Now listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("error while starting TCP listener");

    let router = router::router(state);

    axum::serve(listener, router)
        .await
        .expect("error while starting server");
}
