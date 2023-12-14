use crate::db::DatabaseType;
use crate::network::{communication_types::NodeApiRequest, Node};
use crate::rest::start_rest_api;
use clap::Parser;
use core::panic;
use libp2p::identity::Keypair;
use libp2p::Multiaddr;
use tokio::spawn;
use tokio::sync::mpsc;

mod cli;
mod db;
mod network;
mod rest;
mod utils;

#[tokio::main]
async fn main() {
    env_logger::init();

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
            let ip = {
                match cli_parsed.ip {
                    None => "127.0.0.1".to_string(),
                    Some(a) => a,
                }
            };

            format!("/ip4/{}/tcp/{}", ip, port)
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
    let mut node = Node::new(DatabaseType::create(true), keypair, bootnodes, address);

    // Create channels for node-API communication
    let (request_tx, request_rx) = mpsc::channel::<NodeApiRequest>(32);

    // Clone the request sender for use in the REST API
    let api_request_tx = request_tx.clone();

    // Spawn the node in an asynchronous task with its request receiver
    let node_handle = spawn(async move {
        node.start_node(request_rx).await.unwrap();
    });

    // Spawn the REST API in another asynchronous task with the request sender
    let api_handle = spawn(async move {
        start_rest_api(api_request_tx, rpc_port).await;
    });

    // Optionally, you can wait for both tasks to complete
    let _ = tokio::try_join!(node_handle, api_handle);

    // If you reach here, it means one of the tasks has completed or errored out
    panic!("Node or API stopped unexpectedly");
}
