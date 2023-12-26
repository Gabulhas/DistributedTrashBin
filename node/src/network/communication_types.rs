use crate::network::errors::{DirectorySpecificErrors, KeyAlreadyExists};
use crate::network::jobs::RequestJobState;
use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum InnerRequestValue {
    ObjectRequest { source: Multiaddr },
    ObjectOwnershipSend { value: Vec<u8> },
    KeySearch,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DirectoryRequest {
    pub key: Vec<u8>, //The key used to look up
    pub request_type: InnerRequestValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum InnerResponseValue {
    ReceivedIncomingRequest,
    IsKeyFound(bool),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DirectoryResponse {
    pub key: Vec<u8>,
    pub response_type: InnerResponseValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GetValueResponse {
    Owner(Vec<u8>),             //Either returns the actual value
    Requested(RequestJobState), //Or returns the job number
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
