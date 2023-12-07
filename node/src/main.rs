use crate::db::DatabaseType;
use crate::network::Node;

mod db;
mod network;

#[tokio::main]
async fn main() {
    let mut local_node = Node::new(DatabaseType::create(true));

    match local_node.start_node().await {
        Ok(_) => println!("Node started successfully"),
        Err(e) => eprintln!("Error starting node: {:?}", e),
    }
}
