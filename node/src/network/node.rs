use crate::db::{Database, Value};
use crate::network::node::request_response::ResponseChannel;
use anyhow::{anyhow, bail};
use libp2p::futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic};
use libp2p::kad::{self, store::MemoryStore};
use libp2p::ping;
use libp2p::request_response::{self, cbor, Config, ProtocolSupport};
use libp2p::swarm::{NetworkBehaviour, StreamProtocol, SwarmEvent};
use libp2p::PeerId;
use libp2p::{identity, Swarm};
use serde::{Deserialize, Serialize};
use std::error;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

#[derive(NetworkBehaviour)]
struct DirectoryBehaviour {
    kademlia: kad::Behaviour<MemoryStore>,
    ping: ping::Behaviour,
    gossip: gossipsub::Behaviour,
    request_response: cbor::Behaviour<DirectorySpecificRequest, DirectorySpecificResponse>, // Add other behaviours here
}
pub struct Node {
    swarm: libp2p::Swarm<DirectoryBehaviour>,
    database: Box<dyn Database>,
    gossip_topic: IdentTopic,
}

impl Node {
    pub fn new(database: Box<dyn Database>) -> Self {
        let peer_id = Self::create_local_peer_id();
        let (gossip, gossip_topic) = Self::initialize_gossip();
        let behaviour = Self::initialize_behaviour(peer_id, gossip);

        let swarm = Self::initialize_swarm(behaviour);
        Node {
            swarm,
            database,
            gossip_topic,
        }
    }
    // Creates a local PeerId
    fn create_local_peer_id() -> PeerId {
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        println!("Local peer id: {:?}", local_peer_id);
        local_peer_id
    }

    fn initialize_gossip() -> (gossipsub::Behaviour, IdentTopic) {
        let mut gossip = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::RandomAuthor,
            gossipsub::Config::default(),
        )
        .unwrap();

        let topic = gossipsub::IdentTopic::new("directory-updates");
        gossip.subscribe(&topic).unwrap();

        (gossip, topic)
    }

    fn initialize_behaviour(
        local_peer_id: PeerId,
        gossip: gossipsub::Behaviour,
    ) -> DirectoryBehaviour {
        let store = MemoryStore::new(local_peer_id);
        let kademlia = kad::Behaviour::new(local_peer_id, store);
        let request_response =
            cbor::Behaviour::<DirectorySpecificRequest, DirectorySpecificResponse>::new(
                [(
                    StreamProtocol::new("/my-cbor-protocol"),
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

    pub fn publish_new_key(&mut self, key: String) {
        let topic = self.gossip_topic.clone();

        let message = key.into_bytes(); // Convert the key to bytes

        self.swarm
            .behaviour_mut()
            .gossip
            .publish(topic, message)
            .unwrap();
        // Note: Handle the Result from publish in a way that fits your error handling strategy
    }

    async fn handle_ping_event(&mut self, event: ping::Event) -> Result<(), anyhow::Error> {
        println!("Ping event {:?}", event);
        Ok(())
    }

    async fn handle_kad_event(&mut self, event: kad::Event) -> Result<(), anyhow::Error> {
        match event {
            kad::Event::OutboundQueryProgressed {
                result: kad::QueryResult::GetClosestPeers(Ok(ok)),
                ..
            } => {
                if ok.peers.is_empty() {
                    bail!("Query finished with no closest peers.")
                }
                println!("Query finished with closest peers: {:#?}", ok.peers);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn handle_gossip_event(&mut self, event: gossipsub::Event) -> Result<(), anyhow::Error> {
        match event {
            gossipsub::Event::Message {
                propagation_source,
                message_id,
                message,
            } => {
                println!(
                    "Received gossipsub message {:?} {:?} {:?}",
                    propagation_source, message_id, message
                );
                todo!()
            }
            _ => Ok(()),
        }
    }

    async fn handle_object_ownership_send(
        &mut self,
        key: Vec<u8>,
        value: Vec<u8>,
        channel: ResponseChannel<DirectorySpecificResponse>,
    ) -> Result<(), anyhow::Error> {
        let response = match self.database.get(&key) {
            None => DirectorySpecificResponse::Err(DirectorySpecificErrors::UnexpectedRequest(
                UnexpectedRequest { key },
            )),
            Some(_) => {
                self.database.update(key.clone(), Value::Direct(value));
                DirectorySpecificResponse::Ok
            }
        };

        self.swarm
            .behaviour_mut()
            .request_response
            .send_response(channel, response)
            .map_err(|e| anyhow!("Error sending response: {:?}", e))?;
        Ok(())
    }

    async fn handle_object_request(
        &mut self,
        key: Vec<u8>,
        source: String,
        channel: ResponseChannel<DirectorySpecificResponse>,
        peer: PeerId,
    ) -> Result<(), anyhow::Error> {
        match self.database.get(&key) {
            None => {
                // Implement logic for None case
                todo!();
            }

            Some(value) => {
                self.swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, DirectorySpecificResponse::Ok)
                    .map_err(|e| anyhow!("Error sending response: {:?}", e))?;

                let (next_peer, request_value) = match value {
                    Value::Direct(actual_value) => (
                        PeerId::from_str(&source).expect("Invalid PeerId format"),
                        InnerRequestValue::ObjectOwnershipSend {
                            value: actual_value,
                        },
                    ),
                    Value::Pointer(next_peer) => (
                        PeerId::from_str(&next_peer).expect("Invalid PeerId format"),
                        InnerRequestValue::ObjectRequest { source },
                    ),
                };

                self.swarm.behaviour_mut().request_response.send_request(
                    &next_peer,
                    DirectorySpecificRequest {
                        key: key.clone(),
                        request_type: request_value,
                    },
                );
                self.database.update(key, Value::Pointer(peer.to_string()));
                Ok(())
            }
        }
    }

    async fn handle_request(
        &mut self,
        request: DirectorySpecificRequest,
        peer: PeerId,
        channel: ResponseChannel<DirectorySpecificResponse>,
    ) -> Result<(), anyhow::Error> {
        match request.request_type {
            InnerRequestValue::ObjectRequest { source } => {
                self.handle_object_request(request.key, source, channel, peer)
                    .await
            }
            InnerRequestValue::ObjectOwnershipSend { value } => {
                self.handle_object_ownership_send(request.key, value, channel)
                    .await
            }
        }
    }

    async fn handle_request_response_event(
        &mut self,
        event: request_response::Event<
            DirectorySpecificRequest,
            DirectorySpecificResponse,
            DirectorySpecificResponse,
        >,
    ) -> Result<(), anyhow::Error> {
        match event {
            request_response::Event::Message { message, peer } => {
                match message {
                    request_response::Message::Request {
                        request, channel, ..
                    } => self.handle_request(request, peer, channel).await, // Handle other message types if necessary
                    request_response::Message::Response { .. } => {
                        todo!()
                    }
                }
            } // Handle other event types if necessary
            _ => Ok(()),
        }
    }

    pub async fn start_node(&mut self) -> Result<(), anyhow::Error> {
        loop {
            match self.swarm.select_next_some().await {
                SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {address:?}"),

                SwarmEvent::Behaviour(event) => match event {
                    DirectoryBehaviourEvent::Ping(ping_event) => {
                        // Extract the `ping::Event` from the `ping_event` and pass it to the handler
                        self.handle_ping_event(ping_event).await?
                    }
                    DirectoryBehaviourEvent::Kademlia(event) => {
                        self.handle_kad_event(event).await?
                    }
                    DirectoryBehaviourEvent::Gossip(event) => {
                        self.handle_gossip_event(event).await?
                    }
                    DirectoryBehaviourEvent::RequestResponse(event) => {
                        self.handle_request_response_event(event).await?
                    }
                },
                _ => {}
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum InnerRequestValue {
    ObjectRequest { source: String },
    ObjectOwnershipSend { value: Vec<u8> },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DirectorySpecificRequest {
    key: Vec<u8>, //The key used to look up
    request_type: InnerRequestValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum DirectorySpecificErrors {
    NoValueOrPointerFound(NoValueOrPointerFound),
    UnexpectedRequest(UnexpectedRequest),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum DirectorySpecificResponse {
    Ok,
    Err(DirectorySpecificErrors),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum InnerGossipMessage {
    Add { peer: String },
    Delete,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GossipMessage {
    key: Vec<u8>,
    gossip_type: InnerGossipMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NoValueOrPointerFound {
    peer_id: String,
    key: Vec<u8>,
    source: Vec<u8>,
}

impl fmt::Display for NoValueOrPointerFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "No Value or Pointer {:?}", self)
    }
}

impl error::Error for NoValueOrPointerFound {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UnexpectedRequest {
    key: Vec<u8>,
}

impl fmt::Display for UnexpectedRequest {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "UnexpectedRequest for key {:?}", self.key)
    }
}

impl error::Error for UnexpectedRequest {}
