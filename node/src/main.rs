use crate::db::DatabaseType;
use crate::network::Node;
use crate::rest::start_rest_api;
use std::sync::Arc;
use tokio::sync::Mutex;

mod db;
mod network;
mod rest;

#[tokio::main]
async fn main() {
    let mut local_node = Node::new(DatabaseType::create(true));

    match local_node.start_node().await {
        Ok(_) => println!("Node started successfully"),
        Err(e) => {
            eprintln!("Error starting node: {:?}", e);
            panic!()
        }
    }

    start_rest_api(Arc::new(Mutex::new(local_node))).await
}
