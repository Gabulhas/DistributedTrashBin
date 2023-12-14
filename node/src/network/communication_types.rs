use crate::network::errors::{DirectorySpecificErrors, KeyAlreadyExists};
use crate::network::jobs::JobState;
use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum InnerRequestValue {
    ObjectRequest { source: Multiaddr },
    ObjectOwnershipSend { value: Vec<u8> },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DirectorySpecificRequest {
    pub key: Vec<u8>, //The key used to look up
    pub request_type: InnerRequestValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DirectorySpecificResponse {
    Ok,
    Err(DirectorySpecificErrors),
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
