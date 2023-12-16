#[derive(NetworkBehaviour)]
struct DirectoryBehaviour {
    kademlia: kad::Behaviour<MemoryStore>,
    ping: ping::Behaviour,
    gossip: gossipsub::Behaviour,
    request_response: cbor::Behaviour<ResponseRequestComms, DirectorySpecificResponse>, // Add other behaviours here
}

pub struct NetworkEventHandler {}
