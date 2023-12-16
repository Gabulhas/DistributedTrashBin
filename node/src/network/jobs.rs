use crate::network::errors::DirectorySpecificErrors;
use core::fmt;
use libp2p::Multiaddr;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::Mutex;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestJobState {
    Waiting,
    Finished,
    Failed(DirectorySpecificErrors),
}

impl fmt::Display for RequestJobState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            Self::Waiting => "Waiting",
            Self::Finished => "Finished",
            Self::Failed(_) => "Failed",
        };

        write!(f, "{}", s)
    }
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

pub struct WithRequestInfo {
    pub requester: Multiaddr,
    pub last_peer: PeerId, //Info about the last peer that directly sent the request to the node
}

impl fmt::Display for RequestJob {
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

pub struct RequestJob {
    pub state: RequestJobState,
    pub with_request_info: Option<WithRequestInfo>,
}

pub struct RequestJobManager {
    pub jobs: HashMap<Vec<u8>, Mutex<RequestJob>>,
}

// Upon receiving a peer with a pointer, for simplicity sake we should make it first store the key with the pointer and then call the get_value function and ignore the result
pub enum KeySearchingJob {
    AskingPeers {
        peers: Vec<PeerId>,
        negative_responses: u16,
    },
    AskingGossip, // This should also ask the peers to gossip.
    Failed,       // Can happen that a key actually doesn't exist.
}

pub struct KeySearchingJobManager {
    pub jobs: HashMap<Vec<u8>, KeySearchingJob>,
}
