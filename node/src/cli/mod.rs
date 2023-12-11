use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(name = "DTB Node")]
#[command(version = "1.0")]
#[command(about = "Does some trash distribution", long_about = None)]
pub struct Cli {
    #[arg(short, long, value_name = "FILE")]
    pub key_file: Option<PathBuf>,
    // Optional name to operate on
    #[arg(short, long)]
    pub bootstrap_nodes: Option<Vec<String>>,

    #[arg(short, long)]
    pub multiaddr: Option<String>,
    #[arg(short, long)]
    pub ip: Option<String>,
    #[arg(short, long)]
    pub port: Option<u16>,
    #[arg(short, long)]
    pub rpc_port: u16,
}
