#!/bin/bash


cargo build --release


folder="./target/release"
node="$folder/node"
utils="$folder/utils"


# Function to start a bootnode
start_bootnode() {
    local key_file=$1
    local port=$2
    local rpc_port=$3
    local bootnodes=$4
    # Start a bootnode with key file, port, rpc port, and bootnodes list
    echo "$node --key-file $key_file --port $port --rpc-port $rpc_port $bootnodes "
    $node --key-file "$key_file" --port "$port" --rpc-port "$rpc_port" $bootnodes &
}

# Function to start a regular node
start_node() {
    local port=$1
    local rpc_port=$2
    local bootnodes=$3
    # Start a regular node with port, rpc port, and bootnodes list
    echo "$node --port $port --rpc-port $rpc_port $bootnodes"

    $node --port "$port" --rpc-port "$rpc_port" "$bootnodes" &
}

# Function to build multiaddress for a bootnode
build_multiaddress() {
    local keyfile=$1
    local port=$2
    # Assuming utils show-peer-id outputs the peer ID for a given key file
    local peer_id=$($utils show-peer-id --key-file "$keyfile")
    echo "/ip4/127.0.0.1/tcp/$port/p2p/$peer_id"
}

# Directory containing key files
KEY_DIR="scripts/keys"

# Initialize bootnodes string
BOOTNODES=""


# Build BOOTNODES string
bootnode_count=0
for keyfile in "$KEY_DIR"/*.key; do
    port=$((4000 + bootnode_count))
    BOOTNODE=$(build_multiaddress "$keyfile" "$port")" "
    BOOTNODES="$BOOTNODES --bootstrap-nodes $BOOTNODE"
done


# Start bootnodes with the BOOTNODES string
bootnode_count=0
for keyfile in "$KEY_DIR"/*.key; do
    port=$((4000 + bootnode_count))
    rpc_port=$((3000 + bootnode_count))
    start_bootnode "$keyfile" "$port" "$rpc_port" "$BOOTNODES"
    ((bootnode_count++))
done

exit
# Number of additional nodes to start
NUM_NODES=10
start_port=$((4000 + bootnode_count))

# Start additional nodes
for i in $(seq 1 $NUM_NODES); do
    port=$((start_port + i))
    rpc_port=$((3000 + bootnode_count + i))
    start_node "$port" "$rpc_port" "$BOOTNODES"
done

# Wait for all background processes to finish
wait
