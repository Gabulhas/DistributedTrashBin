use crate::db::Database;
use libp2p::futures::StreamExt;
use libp2p::kad::{self, store::MemoryStore};
use libp2p::ping;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::PeerId;
use libp2p::{identity, Swarm};
use std::error::Error;
use std::time::Duration;

#[derive(NetworkBehaviour)]
struct DirectoryBehaviour {
    kademlia: kad::Behaviour<MemoryStore>,
    ping: ping::Behaviour,
    // Add other behaviours here
}
pub struct Node {
    swarm: libp2p::Swarm<DirectoryBehaviour>,
    database: Box<dyn Database>,
}

impl Node {
    pub fn new(database: Box<dyn Database>) -> Self {
        let peer_id = Self::create_local_peer_id();
        let behaviour = Self::initialize_behaviour(peer_id);
        let swarm = Self::initialize_swarm(behaviour);
        Node { swarm, database }
    }
    // Creates a local PeerId
    fn create_local_peer_id() -> PeerId {
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        println!("Local peer id: {:?}", local_peer_id);
        local_peer_id
    }

    fn initialize_behaviour(local_peer_id: PeerId) -> DirectoryBehaviour {
        let store = MemoryStore::new(local_peer_id);
        let kademlia = kad::Behaviour::new(local_peer_id, store);

        DirectoryBehaviour {
            kademlia,
            ping: ping::Behaviour::default(),
            // ... other behaviours
        }
    }

    fn initialize_swarm(behaviour: DirectoryBehaviour) -> Swarm<DirectoryBehaviour> {
        libp2p::SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::tls::Config::new,
                libp2p::yamux::Config::default,
            )
            .unwrap() // Handle this error appropriately
            .with_behaviour(|_| behaviour)
            .unwrap() // Handle this error appropriately
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(30)))
            .build()
    }

    pub async fn start_node(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            match self.swarm.select_next_some().await {
                SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {address:?}"),
                SwarmEvent::Behaviour(event) => {
                    println!("{event:?}");
                    // Handle events and possibly interact with self.database
                }
                _ => {}
            }
        }
    }
}
