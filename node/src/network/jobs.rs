use crate::network::errors::DirectorySpecificErrors;
use core::fmt;
use dashmap::DashMap;
use libp2p::Multiaddr;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchingJobState {
    AskingPeers {
        peers_asked: usize,
        negative_responses: usize,
    },
    AskingGossip, // This should also ask the peers to gossip.
    Unknown,      // Can happen that a key actually doesn't exist.
}

impl fmt::Display for SearchingJobState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown"),
            Self::AskingPeers {
                peers_asked,
                negative_responses,
            } => {
                write!(
                    f,
                    "AskingPeers(peers_asked: {}, negative_responses: {})",
                    peers_asked, negative_responses
                )
            }
            Self::AskingGossip => write!(f, "AskingGossip"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestJobState {
    Waiting,
    Finished,
    SearchingDirection(SearchingJobState),
    Failed(DirectorySpecificErrors),
}

impl fmt::Display for RequestJobState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Waiting => write!(f, "Waiting"),
            Self::Finished => write!(f, "Finished"),
            Self::Failed(_) => write!(f, "Failed"),
            Self::SearchingDirection(a) => write!(f, "SearchingDirection({})", a),
        }
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
    pub jobs: DashMap<Vec<u8>, Mutex<RequestJob>>,
}
