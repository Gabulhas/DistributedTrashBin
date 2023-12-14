use crate::network::errors::DirectorySpecificErrors;
use core::fmt;
use libp2p::Multiaddr;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::Mutex;
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

pub struct WithRequestInfo {
    pub requester: Multiaddr,
    pub last_peer: PeerId, //Info about the last peer that directly sent the request to the node
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

pub struct Job {
    pub state: JobState,
    pub with_request_info: Option<WithRequestInfo>,
}

pub struct JobManager {
    pub jobs: HashMap<Vec<u8>, Mutex<Job>>,
}
