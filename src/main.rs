mod chat;
mod config;
mod http_server;
mod repository;

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

extern crate config as config_lib;

use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    // Setup logging
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    let mut settings = config_lib::Config::default();
    settings
        .merge(config_lib::File::with_name("config"))
        .unwrap();

    let cfg = settings.try_into::<config::Config>().unwrap();

    let db_cfg = cfg.db;

    let r = repository::new_repo("mongo", db_cfg.clone()).unwrap();
    let repo_mtx = Arc::new(Mutex::new(r));

    let chat_params = chat::Params {
        ws_address: cfg.ws_url,
    };
    let chat = chat::new(chat_params, repo_mtx.clone());
    chat.start();

    // We are forced to use separated repository because chat and http service use different kinds of mutex.
    let r = repository::new_repo("mongo", db_cfg).unwrap();

    let http_server = http_server::new(cfg.http, r);
    http_server.run().await;
}
