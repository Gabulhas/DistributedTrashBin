use crate::db::DatabaseType;
use crate::network::Node;
use crate::rest::start_rest_api;
use clap::Parser;
use core::panic;
use libp2p::identity::Keypair;
use libp2p::Multiaddr;
use std::sync::Arc;
use tokio::sync::Mutex;
mod cli;
mod db;
mod network;
mod rest;
mod utils;

#[tokio::main]
async fn main() {
    let cli_parsed = cli::Cli::parse();
    let keypair = {
        match cli_parsed.key_file {
            Some(p) => match utils::key_from_file(p.to_path_buf()).await {
                Ok(a) => Some(a),
                Err(e) => panic!("{}", e),
            },
            None => None,
        }
    };

    let bootnodes = {
        match cli_parsed.bootstrap_nodes {
            None => Vec::new(),
            Some(a) => a.to_vec(),
        }
    };

    let address = {
        if let Some(multiaddress) = cli_parsed.multiaddr {
            if cli_parsed.port.is_some() {
                panic!("You can either define a port or a multiaddr at a time");
            }
            multiaddress
                .parse::<Multiaddr>()
                .expect("Invalid multiaddress format")
        } else if let Some(port) = cli_parsed.port {
            format!("/ip4/0.0.0.0/tcp/{}", port)
                .parse::<Multiaddr>()
                .expect("Invalid multiaddress format")
        } else {
            panic!("You must either provide a port or a multiaddr");
        }
    };
    start_node(keypair, bootnodes, address, cli_parsed.rpc_port).await;
    panic!("Stopeed unexpectedly")
}

async fn start_node(
    keypair: Option<Keypair>,
    bootnodes: Vec<String>,
    address: Multiaddr,
    rpc_port: u16,
) {
    let node = Node::new(DatabaseType::create(true), keypair, bootnodes, address);

    // Create a shared instance of the node using Arc
    let shared_node = Arc::new(node);

    // Clone the Arc for use in the first task
    let node_clone = shared_node.clone();

    // Spawn the node in an asynchronous task
    let node_handle = tokio::spawn(async move {
        node_clone.start_node().await.unwrap();
    });

    // Clone the Arc for use in the second task (the REST API)
    let api_clone = shared_node.clone();

    // Spawn the REST API in another asynchronous task
    let api_handle = tokio::spawn(async move {
        start_rest_api(api_clone, rpc_port).await;
    });

    // Optionally, you can wait for both tasks to complete
    let _ = tokio::try_join!(node_handle, api_handle);

    // If you reach here, it means one of the tasks has completed or errored out
    panic!("Node or API stopped unexpectedly");
}
