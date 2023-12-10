use crate::db::DatabaseType;
use crate::network::Node;
use crate::rest::start_rest_api;
use clap::Parser;
use libp2p::identity::Keypair;
use libp2p::PeerId;
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
    match &cli_parsed.command {
        cli::Commands::GenerateKey { key_file } => {
            let keypair_res = utils::generate_key_to_file(key_file.to_path_buf()).await;
            match keypair_res {
                Ok(keypair) => println!(
                    "Keypair generated for PeerID {:?}",
                    PeerId::from(keypair.public())
                ),
                Err(e) => panic!("{}", e),
            }
        }
        cli::Commands::Start {
            key_file,
            bootstrap_nodes,
        } => {
            let keypair = {
                match key_file {
                    Some(p) => match utils::key_from_file(p.to_path_buf()).await {
                        Ok(a) => Some(a),
                        Err(e) => panic!("{}", e),
                    },
                    None => None,
                }
            };

            let bootnodes = {
                match bootstrap_nodes {
                    None => Vec::new(),
                    Some(a) => a.to_vec(),
                }
            };

            start_node(keypair, bootnodes).await;
            panic!("Stopeed unexpectedly")
        }
    }
}

async fn start_node(keypair: Option<Keypair>, bootnodes: Vec<String>) {
    let mut local_node = Node::new(DatabaseType::create(true), keypair, bootnodes);

    match local_node.start_node().await {
        Ok(_) => println!("Node started successfully"),
        Err(e) => {
            eprintln!("Error starting node: {:?}", e);
            panic!()
        }
    }

    start_rest_api(Arc::new(Mutex::new(local_node))).await
}
