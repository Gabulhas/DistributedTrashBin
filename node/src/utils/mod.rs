use chrono::Local;
use libp2p::identity::{self, Keypair};
use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{self, AsyncReadExt};

pub async fn key_from_file(file_path: PathBuf) -> io::Result<Keypair> {
    let mut file = File::open(file_path).await?;
    let mut buffer = Vec::new();

    // Read the whole file
    file.read_to_end(&mut buffer).await?;

    identity::Keypair::from_protobuf_encoding(&buffer)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn timestamp_now() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn peer_id_from_multiaddr(multiaddr: Multiaddr) -> Result<PeerId, String> {
    multiaddr
        .iter()
        .find_map(|protocol| {
            if let Protocol::P2p(hash) = protocol {
                PeerId::from_multihash(hash.into()).ok()
            } else {
                None
            }
        })
        .ok_or("Multiaddr does not contain a PeerId".to_string())
}
