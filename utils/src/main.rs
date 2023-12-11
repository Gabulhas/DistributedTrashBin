use clap::{Parser, Subcommand};
use core::panic;
use libp2p::identity::{self, Keypair};
use libp2p::PeerId;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "DTB Utils")]
#[command(version = "1.0")]
#[command(about = "Does some trash distribution", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// does testing things
    GenerateKey {
        #[arg(short, long, value_name = "FILE")]
        key_file: PathBuf,
    },
    ShowKey {
        #[arg(short, long, value_name = "FILE")]
        key_file: PathBuf,
    },
    ShowPeerId {
        #[arg(short, long, value_name = "FILE")]
        key_file: PathBuf,
    },
}
fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::GenerateKey { key_file } => match generate_key_to_file(key_file.to_path_buf()) {
            Err(e) => panic!("{}", e),
            Ok(key) => print_key(key),
        },
        Commands::ShowKey { key_file } => match key_from_file(key_file.to_path_buf()) {
            Ok(key) => print_key(key),
            Err(e) => panic!("{}", e),
        },
        Commands::ShowPeerId { key_file } => match key_from_file(key_file.to_path_buf()) {
            Ok(key) => println!("{}", PeerId::from(key.public()).to_base58()),
            Err(e) => panic!("{}", e),
        },
    }
}

fn print_key(key: Keypair) {
    println!(
        "Peer ID {:?} | Key {:?}",
        PeerId::from(key.public()),
        key.public()
    )
}

pub fn generate_key_to_file(file_path: PathBuf) -> std::result::Result<Keypair, std::io::Error> {
    let key = identity::Keypair::generate_ed25519();
    let encoded = key.to_protobuf_encoding().unwrap();

    let mut file = File::create(file_path.into_os_string())?;

    // Writes some prefix of the byte string, but not necessarily all of it.
    let _ = file.write_all(&encoded);
    Ok(key)
}

pub fn key_from_file(file_path: PathBuf) -> std::result::Result<Keypair, std::io::Error> {
    let mut file = File::open(file_path)?;
    let mut buffer = Vec::new();

    let _ = file.read_to_end(&mut buffer);
    // Read the whole file

    identity::Keypair::from_protobuf_encoding(&buffer)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
