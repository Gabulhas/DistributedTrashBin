use crate::db::{Database, Value};
use crate::network::node::request_response::ResponseChannel;
use anyhow::{anyhow, bail};
use libp2p::futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic};
use libp2p::kad::{self, store::MemoryStore};
use libp2p::multiaddr::Protocol;
use libp2p::ping;
use libp2p::request_response::{self, cbor, Config, ProtocolSupport};
use libp2p::swarm::{NetworkBehaviour, StreamProtocol, SwarmEvent};
use libp2p::Multiaddr;
use libp2p::PeerId;
use libp2p::{identity, Swarm};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::{error, vec};
use tokio::sync::mpsc;
use tokio::sync::{Mutex, RwLock};

#[derive(NetworkBehaviour)]
struct DirectoryBehaviour {
    kademlia: kad::Behaviour<MemoryStore>,
    ping: ping::Behaviour,
    gossip: gossipsub::Behaviour,
    request_response: cbor::Behaviour<DirectorySpecificRequest, DirectorySpecificResponse>, // Add other behaviours here
}

pub struct Node {
    swarm: libp2p::Swarm<DirectoryBehaviour>,
    database: Arc<Mutex<Box<dyn Database>>>,
    gossip_topic: IdentTopic,
    job_manager: Arc<RwLock<JobManager>>,
}

impl Node {
    pub fn new(
        database: Box<dyn Database>,
        local_key_opt: Option<identity::Keypair>,
        bootnodes: Vec<String>,
        local_address: Multiaddr,
    ) -> Self {
        let (mut swarm, gossip_topic) = Self::initialize_libp2p_stuff(local_key_opt, local_address);

        if let Err(e) = Self::add_bootstrap_nodes_to_swarm(&mut swarm, &bootnodes) {
            panic!("{}", e);
        }

        Node {
            swarm,
            database: Arc::new(Mutex::new(database)),
            gossip_topic,
            job_manager: Arc::new(RwLock::new(JobManager {
                jobs: HashMap::new(),
                total_jobs: 0,
            })),
        }
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

    fn initialize_libp2p_stuff(
        local_key_opt: Option<identity::Keypair>,
        local_address: Multiaddr,
    ) -> (Swarm<DirectoryBehaviour>, IdentTopic) {
        let local_key = match local_key_opt {
            Some(local_key) => local_key,
            None => identity::Keypair::generate_ed25519(),
        };

        let local_peer_id = PeerId::from(local_key.public());

        println!("Local peer id: {:?}", local_peer_id);

        let (gossip, gossip_topic) = Self::initialize_gossip(local_key.clone());

        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_key)
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::tls::Config::new,
                libp2p::yamux::Config::default,
            )
            .unwrap() // Handle this error appropriately
            .with_behaviour(|key| Self::initialize_behaviour(key.public().to_peer_id(), gossip))
            .unwrap() // Handle this error appropriately
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(30)))
            .build();

        let _ = swarm.listen_on(local_address);

        (swarm, gossip_topic)
    }

    fn add_bootstrap_nodes_to_swarm(
        swarm: &mut Swarm<DirectoryBehaviour>,
        bootnode_addresses: &[String],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for addr_str in bootnode_addresses {
            println!("Addr_str {}", addr_str);
            let multiaddr: Multiaddr = addr_str.parse()?;
            println!("Multiaddr {}", multiaddr);

            // Extract the PeerId from the Multiaddr
            let peer_id = multiaddr
                .iter()
                .find_map(|protocol| {
                    if let Protocol::P2p(hash) = protocol {
                        PeerId::from_multihash(hash.into()).ok()
                    } else {
                        None
                    }
                })
                .ok_or("Multiaddr does not contain a PeerId")?;

            swarm
                .behaviour_mut()
                .kademlia
                .add_address(&peer_id, multiaddr);
        }

        Ok(())
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
                message,
                ..
            } => {
                let new_key = message.data;

                let mut database = self.database.lock().await;

                if let Some(Value::Pointer(_)) = database.get(&new_key) {
                    Ok(())
                } else {
                    database.insert(new_key, Value::Pointer(propagation_source.to_string()));
                    Ok(())
                }
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
        let mut database = self.database.lock().await;

        //

        let response = match database.get(&key) {
            None => DirectorySpecificResponse::Err(DirectorySpecificErrors::UnexpectedRequest(
                UnexpectedRequest { key },
            )),
            Some(_) => {
                database.update(key.clone(), Value::Direct(value));
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
        let mut database = self.database.lock().await;
        match database.get(&key) {
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
                database.update(key, Value::Pointer(peer.to_string()));
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

    pub async fn start_node(
        &mut self,
        request_rx: mpsc::Receiver<NodeApiRequest>,
        response_tx: mpsc::Sender<NodeApiResponse>,
    ) -> Result<(), anyhow::Error> {
        loop {
            tokio::select! {
                 // Handle network events
                 event = self.swarm.select_next_some() => match event {
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
                 },
            // Handle commands from the channel
                 command = request_rx.recv() => match command {
                     Some(NodeApiRequest::GetValue(key)) => {
                         // Handle GetValue command
                         // Example: let value = self.get_value(key).await;
                     },
                     Some(NodeApiRequest::AddNewValue(key, value)) => {
                         // Handle AddNewValue command
                         // Example: self.add_new_value(key, value).await;
                     },
                     // ... handle other commands ...
                     None => break, // Channel closed
                 },
             }
        }

        /*  loop {
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
        */
    }

    async fn add_new_job(&mut self, key: Vec<u8>) -> JobState {
        let mut job_manager = self.job_manager.write().await;
        job_manager.total_jobs += 1;

        let job_state = JobState::Waiting;

        job_manager.jobs.insert(key, job_state.clone());
        job_state
    }

    async fn start_request_process(&mut self, key: Vec<u8>, next_peer: String) -> JobState {
        let local_peer_id = self.swarm.local_peer_id().to_owned();
        let next_peer = PeerId::from_str(&next_peer).expect("Invalid PeerId format");
        self.swarm.behaviour_mut().request_response.send_request(
            &next_peer,
            DirectorySpecificRequest {
                key: key.clone(),
                request_type: InnerRequestValue::ObjectRequest {
                    source: local_peer_id.to_string(),
                },
            },
        );

        self.add_new_job(key).await
    }

    pub async fn get_value(
        &mut self,
        key: Vec<u8>,
    ) -> Result<GetValueResponse, DirectorySpecificErrors> {
        let job_entry = {
            // Clone the key for immutable borrow and limit the scope of the borrow
            let key_clone = key.clone();
            let job_manager = self.job_manager.read().await;
            job_manager.jobs.get(&key_clone).cloned()
        };

        match job_entry {
            None => {
                // If it has no job related to this, either the node already has the key, or has to request iti
                let next_peer = {
                    let database = self.database.lock().await;

                    match database.get(&key) {
                        None => {
                            return Err(DirectorySpecificErrors::KeyDoesNotExist(KeyDoesNotExist {
                                key,
                            }))
                        }
                        Some(Value::Direct(value)) => return Ok(GetValueResponse::Owner(value)),
                        Some(Value::Pointer(next_peer)) => next_peer.clone(),
                    }
                };

                let job_state = self.start_request_process(key, next_peer).await;
                Ok(GetValueResponse::Requested(job_state))
            }

            // Otherwise, it has either a pending request, a finished request or a failed request
            Some(JobState::Finished) => {
                self.job_manager.write().await.jobs.remove(&key);
                let database = self.database.lock().await;

                if let Some(Value::Direct(value)) = database.get(&key) {
                    Ok(GetValueResponse::Owner(value))
                } else {
                    panic!("Impossible!")
                }
            }
            Some(JobState::Failed(e)) => Err(e.clone()),
            Some(JobState::Waiting) => Ok(GetValueResponse::Requested(JobState::Waiting)),
        }
    }

    fn publish_new_key(&mut self, key: Vec<u8>) {
        let topic = self.gossip_topic.clone();

        self.swarm
            .behaviour_mut()
            .gossip
            .publish(topic, key)
            .unwrap();
    }

    pub async fn add_new_value(
        &mut self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), KeyAlreadyExists> {
        {
            let mut database = self.database.lock().await;

            match database.get(&key) {
                None => database.insert(key.clone(), Value::Direct(value)),
                Some(_) => return Err(KeyAlreadyExists { key }),
            }
        };
        self.publish_new_key(key); //TODO handle this error
        Ok(())
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
pub enum DirectorySpecificErrors {
    NoValueOrPointerFound(NoValueOrPointerFound),
    UnexpectedRequest(UnexpectedRequest),
    KeyDoesNotExist(KeyDoesNotExist),
    KeyAlreadyExists(KeyAlreadyExists),
}

impl fmt::Display for DirectorySpecificErrors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DirectorySpecificErrors::NoValueOrPointerFound(err) => write!(f, "{}", err),
            DirectorySpecificErrors::UnexpectedRequest(err) => write!(f, "{}", err),
            DirectorySpecificErrors::KeyDoesNotExist(err) => write!(f, "{}", err),
            DirectorySpecificErrors::KeyAlreadyExists(err) => write!(f, "{}", err),
            // Handle other variants here as well
        }
    }
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
pub struct NoValueOrPointerFound {
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
pub struct UnexpectedRequest {
    key: Vec<u8>,
}

impl fmt::Display for UnexpectedRequest {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "UnexpectedRequest for key {:?}", self.key)
    }
}

impl error::Error for UnexpectedRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDoesNotExist {
    key: Vec<u8>,
}

impl fmt::Display for KeyDoesNotExist {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Key {:?} does not exist", self.key)
    }
}

impl error::Error for KeyDoesNotExist {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyAlreadyExists {
    key: Vec<u8>,
}

impl fmt::Display for KeyAlreadyExists {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Key {:?} already exists", self.key)
    }
}

impl error::Error for KeyAlreadyExists {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobState {
    Waiting,
    Finished,
    Failed(DirectorySpecificErrors),
}

pub struct JobManager {
    jobs: HashMap<Vec<u8>, JobState>,
    total_jobs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GetValueResponse {
    Owner(Vec<u8>),      //Either returns the actual value
    Requested(JobState), //Or returns the job number
}

enum InnerNodeApiRequest {
    GetValue {
        key: Vec<u8>,
        resp_chan: mpsc::Sender<NodeApiResponse>,
    },
    AddNewValue {
        key: Vec<u8>,
        value: Vec<u8>,
        resp_chan: mpsc::Sender<NodeApiResponse>,
    },
    // ... other request types ...
}

pub struct NodeApiRequest {
    resp_chan: mpsc::Sender<NodeApiResponse>,
    request_type: InnerNodeApiRequest,
}

// Response types sent from the Node back to the API
pub enum NodeApiResponse {
    ValueResult(Result<GetValueResponse, String>), // String for error message
    AddValueResult(Result<(), String>),            // String for error message
}
