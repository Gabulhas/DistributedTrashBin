use libp2p::identity::{self, Keypair};
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};

pub async fn generate_key_to_file(file_path: PathBuf) -> io::Result<Keypair> {
    let key = identity::Keypair::generate_ed25519();
    let encoded = key.to_protobuf_encoding().unwrap();

    let mut file = File::create(file_path.into_os_string()).await?;

    // Writes some prefix of the byte string, but not necessarily all of it.
    file.write_all(&encoded).await?;
    Ok(key)
}

pub async fn key_from_file(file_path: PathBuf) -> io::Result<Keypair> {
    let mut file = File::open(file_path).await?;
    let mut buffer = Vec::new();

    // Read the whole file
    file.read_to_end(&mut buffer).await?;

    identity::Keypair::from_protobuf_encoding(&buffer)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
