use crate::db::{handler::Handler as DatabaseHandler, Database, Value};
use crate::network::communication_types::{
    DirectoryRequest, DirectoryResponse, GetValueResponse, InnerRequestValue, InnerResponseValue,
    NodeApiRequest,
};
use crate::network::errors::{
    DirectorySpecificErrors, KeyAlreadyExists, KeyDoesNotExist, UnexpectedRequest,
};
use crate::network::jobs::{
    RequestJob, RequestJobManager, RequestJobState, SearchingJobState, WithRequestInfo,
};
use crate::network::node::request_response::ResponseChannel;
use crate::network::swarm_and_libp2p::{
    initialize_libp2p_stuff, DirectoryBehaviour, DirectoryBehaviourEvent, DirectoryResponseResult,
};
use crate::utils;
use anyhow::{anyhow, bail};
use dashmap::DashMap;
use libp2p::futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic};
use libp2p::kad;
use libp2p::ping;
use libp2p::request_response::{self};
use libp2p::swarm::SwarmEvent;
use libp2p::Multiaddr;
use libp2p::PeerId;
use libp2p::{identity, Swarm};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

pub struct Node {
    swarm: libp2p::Swarm<DirectoryBehaviour>,
    database_handler: DatabaseHandler,
    gossip_topic: IdentTopic,
    job_manager: Arc<RequestJobManager>,
    address: Multiaddr,
}

impl Node {
    pub fn new(
        database: Box<dyn Database>,
        local_key_opt: Option<identity::Keypair>,
        bootnodes: Vec<String>,
        local_address: Multiaddr,
    ) -> Self {
        let (mut swarm, gossip_topic) =
            initialize_libp2p_stuff(local_key_opt, local_address.clone());

        if let Err(e) = Self::add_bootstrap_nodes_to_swarm(&mut swarm, &bootnodes) {
            panic!("{}", e);
        }

        let database_handler = DatabaseHandler::new(database);

        tokio::spawn(async move {
            database_handler.start().await;
        });

        Node {
            swarm,
            database_handler,
            gossip_topic,
            job_manager: Arc::new(RequestJobManager {
                jobs: DashMap::new(),
            }),
            address: local_address,
        }
    }

    fn print_debug(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S");
        let peer_id = self.swarm.local_peer_id();
        let jobs_count = self.job_manager.jobs.len();

        println!("{timestamp} [Peer ID: {peer_id} | Jobs: {jobs_count}]: {message}");
    }

    fn add_bootstrap_nodes_to_swarm(
        swarm: &mut Swarm<DirectoryBehaviour>,
        bootnode_addresses: &[String],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for addr_str in bootnode_addresses {
            let multiaddr: Multiaddr = addr_str.parse()?;

            let peer_id = utils::peer_id_from_multiaddr(multiaddr.clone()).unwrap();

            // Extract the PeerId from the Multiaddr
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
        Ok(())
    }

    async fn handle_kad_event(&mut self, event: kad::Event) -> Result<(), anyhow::Error> {
        match &event {
            kad::Event::OutboundQueryProgressed { result, .. } => {
                self.print_debug(&format!("Outbound query progressed: {:?}", result));
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
            kad::Event::RoutingUpdated { peer, .. } => {
                self.swarm.behaviour_mut().gossip.add_explicit_peer(peer);
                self.print_debug(&format!("Routing table updated with peer: {:?}", peer));
            }
            kad::Event::RoutablePeer { peer, address } => {
                self.swarm.behaviour_mut().gossip.add_explicit_peer(peer);
                let _ = self.swarm.dial(address.clone());
                println!(
                    "Routable peer discovered: {:?}, address: {:?}",
                    peer, address
                );
            }
            kad::Event::PendingRoutablePeer { peer, address } => {
                let _ = self.swarm.dial(address.clone());
                self.print_debug(&format!(
                    "Pending routable peer: {:?}, address: {:?}",
                    peer, address
                ));
            }
            _ => {}
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
                //TODO in case there was a searching key job previously, delete it and re-execute the "get_value" function to ask for the key

                let new_key = message.data;

                if let Some(Value::Pointer(_)) = self.database_handler.get(new_key.clone()).await {
                    Ok(())
                } else {
                    self.print_debug(&format!("Was told about a new key {:?}", new_key.clone()));
                    self.database_handler
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
        channel: ResponseChannel<DirectoryResponseResult>,
        peer: PeerId,
    ) -> Result<(), anyhow::Error> {
        // ...

        self.print_debug(&format!(
            "Received an OwnershipSend from {} for key {:?}",
            peer,
            key.clone()
        ));
        let response = match self.database_handler.get(key.clone()).await {
            None => {
                self.print_debug(&format!(
                          "Invalid OnwershipSend received from {} for key {:?}: Key not found in database",
                          peer,
                          key.clone()
                        ));
                Err(DirectorySpecificErrors::UnexpectedRequest(
                    UnexpectedRequest { key: key.clone() },
                ))
            }
            Some(_) => {
                self.print_debug(&format!(
                    "OnwershipSend received from {} for key {:?}: I'm currently owner",
                    peer,
                    key.clone()
                ));

                self.database_handler
                    .update(key.clone(), Value::Direct(value));

                Ok(DirectoryResponse {
                    key: key.clone(),
                    response_type: InnerResponseValue::ReceivedIncomingRequest,
                })
            }
        };

        if let Some(current_job) = self.job_manager.jobs.get(&key) {
            let mut job = current_job.lock().await;
            self.print_debug(&format!(
                "OnwershipSend received while having a job for key {:?}: Current job info {}",
                key.clone(),
                job
            ));

            job.state = RequestJobState::Finished;
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
        requester: Multiaddr,
        channel: ResponseChannel<DirectoryResponseResult>,
        peer: PeerId,
    ) -> Result<(), anyhow::Error> {
        let mut update_job = None;
        let mut forward_peer = None; // this is the peer we should first forward to before changing our current link
        let mut update_database_pointer = false;
        let requester_peer_id = utils::peer_id_from_multiaddr(requester.clone()).unwrap();
        let job_manager = self.job_manager.clone();

        match self.database_handler.get(key.clone()).await {
            None => {
                // Implement logic for None case
                // here we can force the nodes that don't know about the key yet, to become a requester
                todo!();
            }
            Some(value) => {
                //If the Node that received a request is either waiting or just received the object (some client asked the node for it, and node is still waiting for that client to refetch)

                if let Some(job) = job_manager.jobs.get(&key) {
                    let job = job.lock().await;
                    let job_state = job.state.clone();

                    self.print_debug(&format!(
                        "Received an object request from {} by {} for {:?}: Currently have a job {}",
                        peer, requester_peer_id, key, job
                    ));

                    // There was no previous request, but we have an unfullfiled request, so the job stays the same (either Finished or Waiting), but since we are waiting for the object to be sent or to receive the object, we have to start forwarding incoming requests
                    match &job.with_request_info {
                        None => {
                            update_job = Some((
                                key.clone(),
                                RequestJob {
                                    state: job_state,
                                    with_request_info: Some(WithRequestInfo {
                                        requester: requester.clone(),
                                        last_peer: peer,
                                    }),
                                },
                            ));
                        }
                        // If it's waiting, any other request tha come through him, we should reconsider the link. This can happen for example, when
                        Some(WithRequestInfo {
                            requester: previous_requester,
                            last_peer: previous_last_peer,
                        }) => {
                            forward_peer = Some(*previous_last_peer);
                            update_job = Some((
                                key.clone(),
                                RequestJob {
                                    state: job_state,
                                    with_request_info: Some(WithRequestInfo {
                                        requester: previous_requester.clone(),
                                        last_peer: peer,
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
                            self.connect_and_send_object(
                                key.clone(),
                                actual_value,
                                requester.clone(),
                            )
                            .await;
                        }
                        Value::Pointer(next_peer) => {
                            self.print_debug(&format!(
                                "Received an object request from {} by {} for {:?}, has no job and no object",
                                peer,utils::peer_id_from_multiaddr(requester.clone()).unwrap(), key
                            ));

                            forward_peer =
                                Some(PeerId::from_str(&next_peer).expect("Invalid PeerId format"));
                        }
                    }
                }

                if let Some((k, n)) = update_job {
                    self.job_manager.jobs.insert(k, Mutex::new(n));
                } else if update_job.is_none() {
                    self.job_manager.jobs.remove(&key);
                }

                if let Some(next_peer) = forward_peer {
                    self.print_debug(&format!(
                        "Forwarding the request for {:?} (Requester {}) to next peer {}",
                        key,
                        utils::peer_id_from_multiaddr(requester.clone()).unwrap(),
                        peer,
                    ));
                    self.swarm.behaviour_mut().request_response.send_request(
                        &next_peer,
                        DirectoryRequest {
                            key: key.clone(),
                            request_type: InnerRequestValue::ObjectRequest { source: requester },
                        },
                    );
                }

                self.swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(
                        channel,
                        Ok(DirectoryResponse {
                            key: key.clone(),
                            response_type: InnerResponseValue::ReceivedIncomingRequest,
                        }),
                    )
                    .map_err(|e| anyhow!("Error sending response: {:?}", e))?;

                if update_database_pointer {
                    self.database_handler
                        .update(key, Value::Pointer(peer.to_string()));
                }

                Ok(())
            }
        }
    }

    async fn handle_key_search_request(
        &mut self,
        key: Vec<u8>,
        _peer: PeerId,
        channel: ResponseChannel<DirectoryResponseResult>,
    ) -> Result<(), anyhow::Error> {
        self.swarm
            .behaviour_mut()
            .request_response
            .send_response(
                channel,
                Ok(DirectoryResponse {
                    key: key.clone(),
                    response_type: InnerResponseValue::IsKeyFound(
                        self.database_handler.get(key.clone()).await.is_some(),
                    ),
                }),
            )
            .map_err(|e| anyhow!("Error sending response: {:?}", e))
    }

    async fn handle_request(
        &mut self,
        request: DirectoryRequest,
        peer: PeerId,
        channel: ResponseChannel<DirectoryResponseResult>,
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
            InnerRequestValue::KeySearch => {
                self.print_debug(&format!(
                    "Received a Key Serach Request from {:?} for key {:?}",
                    peer, request.key
                ));
                self.handle_key_search_request(request.key, peer, channel)
                    .await
            }
        }
    }

    async fn handle_is_key_found_response(
        &mut self,
        key: Vec<u8>,
        peer: PeerId,
        is_found: bool,
    ) -> Result<(), anyhow::Error> {
        let ask_gossip;
        let job_manager = self.job_manager.clone();
        {
            // Limiting the scope of the job_lock
            let job_lock = match job_manager.jobs.get(&key) {
                Some(job) => job,
                None => return Ok(()),
            };

            // Lock the job and access its state
            let mut job = job_lock.lock().await;
            ask_gossip = if let RequestJobState::SearchingDirection(search_state) = &mut job.state {
                if is_found {
                    self.database_handler
                        .insert(key.clone(), Value::Pointer(peer.to_string()));

                    // If key is found, proceed to get value
                    drop(job); // Explicitly drop the lock before the await point
                    return match self.get_value(key).await {
                        Ok(_) => Ok(()),
                        Err(a) => Err(anyhow::Error::msg(a.to_string())),
                    };
                } else {
                    // Update state for negative response
                    if let SearchingJobState::AskingPeers {
                        ref mut negative_responses,
                        peers_asked,
                    } = search_state
                    {
                        *negative_responses += 1;
                        *negative_responses >= *peers_asked
                    } else {
                        false
                    }
                }
            } else {
                false
            };
        } // job is dropped here, ending the immutable borrow

        // ask_gossip is now used outside the scope of the job borrow
        if ask_gossip {
            self.ask_gossip_for_key(key); // Assuming this is an async function
        }

        Ok(())
    }

    async fn handle_response(
        &mut self,
        response: DirectoryResponseResult,
        peer: PeerId,
    ) -> Result<(), anyhow::Error> {
        let inner = match response {
            Err(a) => {
                // Convert DirectorySpecificErrors to a string and then to anyhow::Error
                return Err(anyhow::Error::msg(a.to_string()));
            }
            Ok(inner) => inner,
        };

        match inner.response_type {
            InnerResponseValue::ReceivedIncomingRequest => Ok(()),
            InnerResponseValue::IsKeyFound(is_found) => {
                self.handle_is_key_found_response(inner.key, peer, is_found)
                    .await
            }
        }
    }

    async fn handle_request_response_event(
        &mut self,
        event: request_response::Event<DirectoryRequest, DirectoryResponseResult>,
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
                        self.handle_response(response, peer).await
                    }
                }
            } // Handle other event types if necessary
            _ => {
                self.print_debug(&format!("Get a Request Response Event {:?}", event));
                Ok(())
            }
        }
    }

    pub async fn start_node(
        &mut self,
        mut api_command_rx: mpsc::Receiver<NodeApiRequest>,
    ) -> Result<(), anyhow::Error> {
        loop {
            tokio::select! {
              //TODO: any even shouldn't block the loop

                event = self.swarm.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } => self.print_debug(&format!("Listening on {:?}", address)),
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => self.print_debug(&format!("Connection established with peer: {:?}", peer_id)),
                    SwarmEvent::ConnectionClosed { peer_id, .. } => self.print_debug(&format!("Connection closed with peer: {:?}", peer_id)),
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

    async fn start_new_job(&mut self, key: Vec<u8>) -> RequestJobState {
        let job_state = RequestJobState::Waiting;
        self.job_manager.jobs.insert(
            key,
            Mutex::new(RequestJob {
                state: job_state.clone(),
                with_request_info: None,
            }),
        );
        job_state
    }

    async fn start_request_process(&mut self, key: Vec<u8>, next_peer: String) -> RequestJobState {
        let next_peer = PeerId::from_str(&next_peer).expect("Invalid PeerId format");
        let address_with_peer_id = self
            .address
            .clone()
            .with_p2p(*self.swarm.local_peer_id())
            .unwrap();
        self.swarm.behaviour_mut().request_response.send_request(
            &next_peer,
            DirectoryRequest {
                key: key.clone(),
                request_type: InnerRequestValue::ObjectRequest {
                    source: address_with_peer_id,
                },
            },
        );

        self.start_new_job(key).await
    }

    async fn start_timeout_thread_for_peer_asking(
        &mut self,
        key: Vec<u8>,
        job_to_timeout: Mutex<RequestJob>,
    ) -> Option<()> {
        // HMMMMM
        let job_manager = self.job_manager.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(10)).await;
            if !job_manager.jobs.contains_key(&key) {
                return;
            }

            // Lock the job and access its state
            let mut job = job_to_timeout.lock().await;
            match job.state {
                RequestJobState::SearchingDirection(SearchingJobState::AskingPeers { .. }) => {
                    job.state = RequestJobState::Failed(DirectorySpecificErrors::KeyDoesNotExist(
                        KeyDoesNotExist { key: key.clone() },
                    ));
                }
                _ => return,
            }
        });
        Some(())
    }

    async fn find_key_direction_peers(
        &mut self,
        key: Vec<u8>,
    ) -> Result<SearchingJobState, DirectorySpecificErrors> {
        // This is used in case a node doens't know about a key (yet)
        // TODO: add something about when a key is not yet added but receives a key gossipfrom the network
        // imagine that a new key was added on A, and a user asked node B for that key, yet the gossip hasn't reached node B
        // we should cancel the find_key_direction_peers job
        // Also, do something for when a node is looking for the direction but receives a request too. Maybe, the node that just creates a "waiting" request

        //Ask Peers -> Gossip -> Assume it doens't exist
        let job_state = SearchingJobState::AskingPeers {
            peers_asked: self.swarm.connected_peers().count(),
            negative_responses: 0,
        };

        self.job_manager.jobs.insert(
            key.clone(),
            Mutex::new(RequestJob {
                state: RequestJobState::SearchingDirection(job_state.clone()),
                with_request_info: None,
            }),
        );

        let peer_ids: Vec<_> = self.swarm.connected_peers().cloned().collect();

        for peer_id in peer_ids {
            let request = DirectoryRequest {
                key: key.clone(),
                request_type: InnerRequestValue::KeySearch,
            };

            // Mutable borrow of `self.swarm` is now allowed
            self.swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer_id, request);
        }

        //TODO: Create a timeout to then gossip and cancel the peer asking

        Ok(job_state)
    }

    pub async fn get_value(
        &mut self,
        key: Vec<u8>,
    ) -> Result<GetValueResponse, DirectorySpecificErrors> {
        // Clone the key for immutable borrow and limit the scope of the borrow

        let mut remove_job = false;
        let mut send_object = None;
        let job_manager = self.job_manager.clone();

        let result = match job_manager.jobs.get(&key) {
            None => {
                // If it has no job related to this, either the node already has the key, or has to request iti

                let next_peer = {
                    match self.database_handler.get(key.clone()).await {
                        None => match self.find_key_direction_peers(key).await {
                            Ok(a) => {
                                return Ok(GetValueResponse::Requested(
                                    RequestJobState::SearchingDirection(a),
                                ))
                            }
                            Err(e) => return Err(e),
                        },
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
                let job_state = self.start_request_process(key.clone(), next_peer).await;
                Ok(GetValueResponse::Requested(job_state))
            }

            // Whe a user re-fetches the value, and it's either there, or just waiting to be there
            Some(job) => {
                let job = job.lock().await;

                match &job.state {
                    // Otherwise, it has either a pending request, a finished request or a failed request
                    // Check if has any peer, and send
                    //LOCK/lock/timelock/value lock
                    RequestJobState::Finished => {
                        self.print_debug(
                            "Get Value was executed. Node has the object and job is finished",
                        );
                        remove_job = true;

                        let actual_data = {
                            if let Some(Value::Direct(value)) =
                                self.database_handler.get(key.clone()).await
                            {
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
                            send_object = Some((actual_data.clone(), requester.clone()));

                            self.database_handler
                                .update(key.clone(), Value::Pointer(last_peer.to_string()))
                                .await;
                        }

                        Ok(GetValueResponse::Owner(actual_data))
                    }
                    RequestJobState::Failed(e) => Err(e.clone()),
                    RequestJobState::Waiting => {
                        Ok(GetValueResponse::Requested(RequestJobState::Waiting))
                    }
                    RequestJobState::SearchingDirection(a) => {
                        todo!(
                            "{}",
                            &format!(
                            "Someone Requested a key I'm looking for. Current state of search {}"
                        , a.clone())
                        )
                    }
                }
            }
        };

        if let Some((actual_data, requester)) = send_object {
            self.connect_and_send_object(key.clone(), actual_data, requester)
                .await;
        }

        if remove_job {
            self.job_manager.jobs.remove(&key);
        }
        result
    }

    async fn connect_and_send_object(
        &mut self,
        key: Vec<u8>,
        actual_data: Vec<u8>,
        requester: Multiaddr,
    ) {
        let requester_peer_id = utils::peer_id_from_multiaddr(requester.clone()).unwrap();

        self.swarm.dial(requester).unwrap();

        while !self
            .swarm
            .behaviour()
            .request_response
            .is_connected(&requester_peer_id)
        {
            sleep(Duration::from_millis(100)).await; // Adjust the delay duration as needed
            self.print_debug("Waiting for connection")
        }

        self.swarm.behaviour_mut().request_response.send_request(
            &requester_peer_id,
            DirectoryRequest {
                key,
                request_type: InnerRequestValue::ObjectOwnershipSend { value: actual_data },
            },
        );
    }

    fn publish_new_key(&mut self, key: Vec<u8>) {
        let topic = self.gossip_topic.clone();

        self.swarm
            .behaviour_mut()
            .gossip
            .publish(topic, key)
            .unwrap();
    }

    fn ask_gossip_for_key(&mut self, key: Vec<u8>) {
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

        match self.database_handler.get(key.clone()).await {
            None => {
                self.database_handler
                    .insert(key.clone(), Value::Direct(value))
                    .await
            }
            Some(_) => return Err(KeyAlreadyExists { key }),
        }
        self.publish_new_key(key);
        Ok(())
    }
}
