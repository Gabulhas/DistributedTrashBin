use crate::network::communication_types::{DirectoryRequest, DirectoryResponse};
use crate::network::errors::DirectorySpecificErrors;
use libp2p::gossipsub::{self, IdentTopic};
use libp2p::identity;
use libp2p::kad::{self, store::MemoryStore};
use libp2p::ping;
use libp2p::request_response::{cbor, Config, ProtocolSupport};
use libp2p::swarm::NetworkBehaviour;
use libp2p::Multiaddr;
use libp2p::PeerId;
use libp2p::StreamProtocol;
use libp2p::Swarm;
use tokio::time::Duration;

pub type DirectoryResponseResult = Result<DirectoryResponse, DirectorySpecificErrors>;

#[derive(NetworkBehaviour)]
pub struct DirectoryBehaviour {
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub ping: ping::Behaviour,
    pub gossip: gossipsub::Behaviour,
    pub request_response: cbor::Behaviour<DirectoryRequest, DirectoryResponseResult>,
}

fn initialize_gossip(keypair: identity::Keypair) -> (gossipsub::Behaviour, IdentTopic) {
    let config = gossipsub::ConfigBuilder::default()
        .build()
        .expect("Valid Gossipsub config");

    let mut gossip = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(keypair.clone()),
        config,
    )
    .expect("Correct Gossipsub instantiation");

    let topic = gossipsub::IdentTopic::new("directory-updates");
    gossip
        .subscribe(&topic)
        .expect("Topic subscription to succeed");

    (gossip, topic)
}

fn initialize_behaviour(local_peer_id: PeerId, gossip: gossipsub::Behaviour) -> DirectoryBehaviour {
    let store = MemoryStore::new(local_peer_id);
    let kademlia = kad::Behaviour::new(local_peer_id, store);
    let request_response = cbor::Behaviour::<DirectoryRequest, DirectoryResponseResult>::new(
        [(
            StreamProtocol::new("/request-response-directory"),
            ProtocolSupport::Full,
        )],
        Config::default(),
    );

    DirectoryBehaviour {
        kademlia,
        ping: ping::Behaviour::default(),
        gossip,
        request_response,
    }
}

pub fn initialize_libp2p_stuff(
    local_key_opt: Option<identity::Keypair>,
    local_address: Multiaddr,
) -> (Swarm<DirectoryBehaviour>, IdentTopic) {
    let local_key = match local_key_opt {
        Some(local_key) => local_key,
        None => identity::Keypair::generate_ed25519(),
    };

    let (gossip, gossip_topic) = initialize_gossip(local_key.clone());

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::tls::Config::new,
            libp2p::yamux::Config::default,
        )
        .unwrap() // Handle this error appropriately
        .with_behaviour(|key| initialize_behaviour(key.public().to_peer_id(), gossip))
        .unwrap() // Handle this error appropriately
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(30)))
        .build();

    let _ = swarm.listen_on(local_address);

    (swarm, gossip_topic)
}
