import subprocess
import os
import glob
import sys

# Directory for the built project and scripts
folder = "./target/release"
node_executable = os.path.join(folder, "node")
utils_executable = os.path.join(folder, "utils")
key_dir = "live_tests/keys"

# Node class to hold node details


class Node:
    def __init__(self, port, rpc_port, key_file=None, process=None):
        print(f"Node {port} | {rpc_port}")
        self.port = port
        self.rpc_port = rpc_port
        self.key_file = key_file
        self.process = process


def build_multiaddress(keyfile, port):
    """Builds a multiaddress for a bootnode."""
    peer_id = subprocess.check_output(
        [utils_executable, "show-peer-id", "--key-file", keyfile]).decode(sys.stdout.encoding)
    print(type(peer_id))
    return f"/ip4/127.0.0.1/tcp/{port}/p2p/{peer_id.strip()}"


def start_node(is_bootnode, port, rpc_port, bootnodes, key_file=None):
    """Starts a bootnode or a regular node."""
    cmd = [node_executable, "--port",
           str(port), "--rpc-port", str(rpc_port)] + bootnodes.split()
    if is_bootnode:
        cmd += ["--key-file", key_file]
    process = subprocess.Popen(cmd)
    return Node(port, rpc_port, key_file, process)


def initialize_bootnodes():
    """Initializes and starts bootnodes."""
    bootnodes = ""
    nodes = []
    bootnode_count = 0

    for keyfile in glob.glob(os.path.join(key_dir, "*.key")):
        port = 4000 + bootnode_count
        bootnode = build_multiaddress(keyfile, port)
        bootnodes += f" --bootstrap-nodes {bootnode}"
        bootnode_count += 1

    for i, keyfile in enumerate(glob.glob(os.path.join(key_dir, "*.key"))):
        port = 4000 + i
        rpc_port = 3000 + i
        node = start_node(True, port, rpc_port, bootnodes, key_file=keyfile)
        nodes.append(node)

    return nodes, bootnodes


def start_additional_nodes(bootnode_count, bootnodes):
    """Starts additional regular nodes."""
    nodes = []
    num_nodes = 10
    start_port = 4000 + bootnode_count

    for i in range(num_nodes):
        port = start_port + i
        rpc_port = 3000 + bootnode_count + i
        node = start_node(False, port, rpc_port, bootnodes)
        nodes.append(node)

    return nodes


def main():
    bootnodes, bootnodes_str = initialize_bootnodes()
    regular_nodes = start_additional_nodes(len(bootnodes), bootnodes_str)
    all_nodes = bootnodes + regular_nodes

    # Wait for all nodes to finish
    for node in all_nodes:
        node.process.wait()


if __name__ == "__main__":
    main()
