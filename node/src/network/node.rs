use crate::db::{Database, Value};
use crate::network::node::request_response::ResponseChannel;
use crate::utils;
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
//use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;
use std::{error, vec};
use tokio::sync::{mpsc, Mutex};

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
    job_manager: JobManager,
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
            database,
            gossip_topic,
            job_manager: JobManager {
                jobs: HashMap::new(),
            },
        }
    }

    fn print_debug(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S");
        let peer_id = self.swarm.local_peer_id();
        let jobs_count = self.job_manager.jobs.len();

        println!("{timestamp} [Peer ID: {peer_id} | Jobs: {jobs_count}]: {message}");
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

    fn initialize_libp2p_stuff(
        local_key_opt: Option<identity::Keypair>,
        local_address: Multiaddr,
    ) -> (Swarm<DirectoryBehaviour>, IdentTopic) {
        let local_key = match local_key_opt {
            Some(local_key) => local_key,
            None => identity::Keypair::generate_ed25519(),
        };

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
            let multiaddr: Multiaddr = addr_str.parse()?;

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
            if *swarm.local_peer_id() == peer_id {
                continue;
            }

            swarm
                .behaviour_mut()
                .kademlia
                .add_address(&peer_id, multiaddr);
        }

        Ok(())
    }

    async fn handle_ping_event(&mut self, _event: ping::Event) -> Result<(), anyhow::Error> {
        // println!(
        //
        //     "{} | {}Ping event {:?}",
        //     utils::timestamp_now(),
        //     self.swarm.local_peer_id(),
        //     event
        // );
        Ok(())
    }

    async fn handle_kad_event(&mut self, event: kad::Event) -> Result<(), anyhow::Error> {
        match &event {
            kad::Event::OutboundQueryProgressed { result, .. } => {
                self.print_debug(&format!(
                    "{} | {}Outbound query progressed: {:?}",
                    utils::timestamp_now(),
                    self.swarm.local_peer_id(),
                    result
                ));
                if let kad::QueryResult::GetClosestPeers(Ok(ok)) = result {
                    if ok.peers.is_empty() {
                        bail!("Query finished with no closest peers.")
                    }
                    println!(
                        "{} | {}Closest peers: {:#?}",
                        utils::timestamp_now(),
                        self.swarm.local_peer_id(),
                        ok.peers
                    );
                }
            }
            //kad::Event::UnroutablePeer { peer } => {
            //    println!(
            //        "{} | {}Discovered unroutable peer: {:?}",
            //        utils::timestamp_now(),
            //        self.swarm.local_peer_id(),
            //        peer
            //    );
            //}
            kad::Event::RoutingUpdated { peer, .. } => {
                self.swarm.behaviour_mut().gossip.add_explicit_peer(peer);
                self.print_debug(&format!(
                    "{} | {}Routing table updated with peer: {:?}",
                    utils::timestamp_now(),
                    self.swarm.local_peer_id(),
                    peer
                ));
            }
            //kad::Event::RoutablePeer { peer, address } => {
            //    println!(
            //        "Routable peer discovered: {:?}, address: {:?}",
            //        peer, address
            //    );
            //}
            //kad::Event::PendingRoutablePeer { peer, address } => {
            //    println!(
            //        "{} | {}Pending routable peer: {:?}, address: {:?}",
            //        utils::timestamp_now(),
            //        self.swarm.local_peer_id(),
            //        peer,
            //        address
            //    );
            //}
            // Log other event types here
            _ => {
                //                println!(
                //                    "{} | {}Other Kademlia event: {:?}",
                //                    utils::timestamp_now(),
                //                    self.swarm.local_peer_id(),
                //                    event
                //                );
            }
        }
        Ok(())
    }

    async fn handle_gossip_event(&mut self, event: gossipsub::Event) -> Result<(), anyhow::Error> {
        match event {
            gossipsub::Event::Message {
                propagation_source,
                message,
                ..
            } => {
                let new_key = message.data;

                if let Some(Value::Pointer(_)) = self.database.get(&new_key) {
                    Ok(())
                } else {
                    self.print_debug(&format!("Was told about a new key {:?}", new_key.clone()));
                    self.database
                        .insert(new_key, Value::Pointer(propagation_source.to_string()));
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
        peer: PeerId,
    ) -> Result<(), anyhow::Error> {
        // ...

        self.print_debug(&format!(
            "Received an OwnershipSend from {} for key {:?}",
            peer,
            key.clone()
        ));
        let response = match self.database.get(&key) {
            None => {
                self.print_debug(&format!(
                          "Invalid OnwershipSend received from {} for key {:?}: Key not found in database",
                          peer,
                          key.clone()
                        ));
                DirectorySpecificResponse::Err(DirectorySpecificErrors::UnexpectedRequest(
                    UnexpectedRequest { key: key.clone() },
                ))
            }
            Some(_) => {
                self.print_debug(&format!(
                    "OnwershipSend received from {} for key {:?}: I'm currently owner",
                    peer,
                    key.clone()
                ));

                self.database.update(key.clone(), Value::Direct(value));

                DirectorySpecificResponse::Ok
            }
        };

        if let Some(current_job) = self.job_manager.jobs.get(&key) {
            let mut job = current_job.lock().await;
            self.print_debug(&format!(
                "OnwershipSend received while having a job for key {:?}: Current job info {}",
                key.clone(),
                job
            ));

            job.state = JobState::Finished;
        } else {
            panic!(
                "Impossible! Received an Ownership from {} for {:?} but didn't have any jobs",
                peer, key
            )
        }

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
        let mut update_job = None;
        let mut forward_peer = None; // this is the peer we should first forward to before changing our current link
        let mut update_database_pointer = false;

        match self.database.get(&key) {
            None => {
                // Implement logic for None case
                // here we can force the nodes that don't know about the key yet, to become a requester
                todo!();
            }
            Some(value) => {
                //If the Node that received a request is either waiting or just received the object (some client asked the node for it, and node is still waiting for that client to refetch)

                if let Some(job) = self.job_manager.jobs.get(&key) {
                    let job = job.lock().await;
                    let job_state = job.state.clone();

                    self.print_debug(&format!(
                        "Received an object request from {} by {} for {:?}: Currently have a job {}",
                        peer, source, key, job
                    ));

                    // There was no previous request, but we have an unfullfiled request, so the job stays the same (either Finished or Waiting), but since we are waiting for the object to be sent or to receive the object, we have to start forwarding incoming requests
                    match &job.with_request_info {
                        None => {
                            update_job = Some((
                                key.clone(),
                                Job {
                                    state: job_state,
                                    with_request_info: Some(WithRequestInfo {
                                        requester: source.clone(),
                                        last_peer: peer.to_string(),
                                    }),
                                },
                            ));
                        }
                        // If it's waiting, any other request tha come through him, we should reconsider the link. This can happen for example, when
                        Some(WithRequestInfo {
                            requester: previous_requester,
                            last_peer: previous_last_peer,
                        }) => {
                            forward_peer = Some(previous_last_peer.clone());
                            update_job = Some((
                                key.clone(),
                                Job {
                                    state: job_state,
                                    with_request_info: Some(WithRequestInfo {
                                        requester: previous_requester.to_string(),
                                        last_peer: peer.to_string(),
                                    }),
                                },
                            ));
                        }
                    }

                // If the Node received a request and it's not waiting (just idle) or is just owner (with no request), then we just send it directly
                } else {
                    update_database_pointer = true;

                    match value {
                        Value::Direct(actual_value) => {
                            self.print_debug(&format!(
                                "Received an object request from {} by {} for {:?}, has no job but has the object. Sending",
                                peer, source, key
                            ));

                            //HERE IT'S SENDING THE OBJECt
                            let response_id =
                                self.swarm.behaviour_mut().request_response.send_request(
                                    &PeerId::from_str(&source).expect("Invalid PeerId format"),
                                    DirectorySpecificRequest {
                                        key: key.clone(),
                                        request_type: InnerRequestValue::ObjectOwnershipSend {
                                            value: actual_value.clone(),
                                        },
                                    },
                                );
                            self.print_debug(&format!(
                                "Sending object to {}. Response ID is {}",
                                source, response_id
                            ));
                        }
                        Value::Pointer(next_peer) => {
                            self.print_debug(&format!(
                                "Received an object request from {} by {} for {:?}, has no job and no object",
                                peer, source, key
                            ));

                            forward_peer = Some(next_peer);
                        }
                    }
                }

                if let Some((k, n)) = update_job {
                    self.job_manager.jobs.insert(k, Mutex::new(n));
                } else if update_job.is_none() {
                    self.job_manager.jobs.remove(&key);
                }

                if let Some(next_peer) = forward_peer {
                    self.swarm.behaviour_mut().request_response.send_request(
                        &PeerId::from_str(&next_peer).expect("Invalid PeerId format"),
                        DirectorySpecificRequest {
                            key: key.clone(),
                            request_type: InnerRequestValue::ObjectRequest { source },
                        },
                    );
                }

                self.swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, DirectorySpecificResponse::Ok)
                    .map_err(|e| anyhow!("Error sending response: {:?}", e))?;

                if update_database_pointer {
                    self.database.update(key, Value::Pointer(peer.to_string()));
                }

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
                self.print_debug(&format!(
                    "Received a Ownership Send {:?} for key {:?}",
                    peer, request.key
                ));
                self.handle_object_ownership_send(request.key, value, channel, peer)
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
                    request_response::Message::Response {
                        response,
                        request_id,
                    } => {
                        self.print_debug(&format!("Reponse {:?} with ID {}", response, request_id));
                        Ok(())
                    }
                }
            } // Handle other event types if necessary
            _ => Ok(()),
        }
    }

    pub async fn start_node(
        &mut self,
        mut api_command_rx: mpsc::Receiver<NodeApiRequest>,
    ) -> Result<(), anyhow::Error> {
        loop {
            tokio::select! {
                // Handle network events
                event = self.swarm.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } => println!("{} | {}Listening on {address:?}", utils::timestamp_now(), self.swarm.local_peer_id()),
                    SwarmEvent::Behaviour(event) => self.handle_network_event(event).await?,
                    _ => {},
                },

                // Handle API commands
                api_command = api_command_rx.recv() => match api_command {
                    Some(NodeApiRequest::GetValue { key, resp_chan }) => {
                        // Process GetValue command
                        let result = self.get_value(key).await; // Example function call
                        let _ = resp_chan.send(result).await;
                    },
                    Some(NodeApiRequest::AddNewValue { key, value, resp_chan }) => {
                        // Process AddNewValue command
                        let result = self.add_new_value(key, value).await; // Example function call
                        let _ = resp_chan.send(result).await;
                    },
                    // ... handle other types of API commands ...
                    None => break, // Channel closed
                },
            }
        }

        Ok(())
    }

    // Separate function to handle network events
    async fn handle_network_event(
        &mut self,
        event: DirectoryBehaviourEvent,
    ) -> Result<(), anyhow::Error> {
        match event {
            DirectoryBehaviourEvent::Ping(ping_event) => self.handle_ping_event(ping_event).await,
            DirectoryBehaviourEvent::Kademlia(kad_event) => self.handle_kad_event(kad_event).await,
            DirectoryBehaviourEvent::Gossip(gossip_event) => {
                self.handle_gossip_event(gossip_event).await
            }
            DirectoryBehaviourEvent::RequestResponse(req_res_event) => {
                self.handle_request_response_event(req_res_event).await
            } // ... other network event handlers ...
        }
    }

    async fn add_new_job(&mut self, key: Vec<u8>) -> JobState {
        let job_state = JobState::Waiting;
        self.job_manager.jobs.insert(
            key,
            Mutex::new(Job {
                state: job_state.clone(),
                with_request_info: None,
            }),
        );
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
        // Clone the key for immutable borrow and limit the scope of the borrow
        //TODO: Here, if a job entry is final and it's retrieved, we should send the object to the requester (if any) and replace it in the database. If no requester is present, nothing is done

        match self.job_manager.jobs.get(&key) {
            None => {
                // If it has no job related to this, either the node already has the key, or has to request iti

                let next_peer = {
                    match self.database.get(&key) {
                        None => {
                            return Err(DirectorySpecificErrors::KeyDoesNotExist(KeyDoesNotExist {
                                key,
                            }))
                        }
                        Some(Value::Direct(value)) => {
                            self.print_debug(
                                "Get Value was executed. Node has the object. Sending directly.",
                            );
                            return Ok(GetValueResponse::Owner(value));
                        }
                        Some(Value::Pointer(next_peer)) => next_peer.clone(),
                    }
                };

                self.print_debug(&format!(
                    "Get Value was executed. Node doesn't have the object. requesting {}",
                    next_peer
                ));
                let job_state = self.start_request_process(key, next_peer).await;
                //TODO: there's something to do here I bet
                Ok(GetValueResponse::Requested(job_state))
            }

            // Whe a user re-fetches the value, and it's either there, or just waiting to be there
            Some(job) => {
                let job = job.lock().await;

                match &job.state {
                    // Otherwise, it has either a pending request, a finished request or a failed request
                    // Check if has any peer, and send
                    //LOCK/lock/timelock/value lock
                    JobState::Finished => {
                        self.print_debug(
                            "Get Value was executed. Node has the object and job is finished",
                        );

                        let actual_data = {
                            if let Some(Value::Direct(value)) = self.database.get(&key) {
                                value
                            } else {
                                panic!("Impossible!")
                            }
                        };

                        if let Some(WithRequestInfo {
                            requester,
                            last_peer,
                        }) = &job.with_request_info
                        {
                            self.swarm.behaviour_mut().request_response.send_request(
                                &PeerId::from_str(requester).expect("Invalid PeerId format"),
                                DirectorySpecificRequest {
                                    key: key.clone(),
                                    request_type: InnerRequestValue::ObjectOwnershipSend {
                                        value: actual_data.clone(),
                                    },
                                },
                            );
                            self.database
                                .update(key, Value::Pointer(last_peer.to_string()));
                        }

                        Ok(GetValueResponse::Owner(actual_data))
                    }
                    JobState::Failed(e) => Err(e.clone()),
                    JobState::Waiting => Ok(GetValueResponse::Requested(JobState::Waiting)),
                }
            }
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
        self.print_debug(&format!(
            "New Key-Value added {:?} {:?}",
            key,
            value.clone()
        ));

        match self.database.get(&key) {
            None => self.database.insert(key.clone(), Value::Direct(value)),
            Some(_) => return Err(KeyAlreadyExists { key }),
        }
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

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            Self::Waiting => "Waiting",
            Self::Finished => "Finished",
            Self::Failed(_) => "Failed",
        };

        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobState {
    Waiting,
    Finished,
    Failed(DirectorySpecificErrors),
}
impl fmt::Display for WithRequestInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Requester {}. Last Peer {}",
            self.requester, self.last_peer
        )
    }
}

struct WithRequestInfo {
    requester: String,
    last_peer: String, //Info about the last peer that directly sent the request to the node
}

impl fmt::Display for Job {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let with_request_string = {
            match &self.with_request_info {
                Some(a) => a.to_string(),
                None => "No Request Info".to_string(),
            }
        };
        write!(f, "State: {}. {}", self.state, with_request_string)
    }
}

struct Job {
    state: JobState,
    with_request_info: Option<WithRequestInfo>,
}

pub struct JobManager {
    jobs: HashMap<Vec<u8>, Mutex<Job>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GetValueResponse {
    Owner(Vec<u8>),      //Either returns the actual value
    Requested(JobState), //Or returns the job number
}

pub enum NodeApiRequest {
    GetValue {
        key: Vec<u8>,
        resp_chan: mpsc::Sender<Result<GetValueResponse, DirectorySpecificErrors>>,
    },
    AddNewValue {
        key: Vec<u8>,
        value: Vec<u8>,
        resp_chan: mpsc::Sender<Result<(), KeyAlreadyExists>>,
    },
}
