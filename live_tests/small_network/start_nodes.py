from typing import List
import subprocess
import threading
import os
import glob
import http.server
import socketserver
import json
import sys

# Directory for the built project and scripts
folder = "./target/release"
node_executable = os.path.join(folder, "node")
utils_executable = os.path.join(folder, "utils")
key_dir = "live_tests/keys"
all_nodes = []


class NodeInfoProvider(http.server.SimpleHTTPRequestHandler):

    def do_GET(self):
        # Set response status code
        self.send_response(200)

        # Set headers
        self.send_header('Content-type', 'application/json')
        self.end_headers()

        # Send JSON response
        response = [x.to_json_dict() for x in all_nodes]
        print(response)
        self.wfile.write(json.dumps(response).encode())


class Node:
    def __init__(self, port, rpc_port, key_file=None, process=None):
        print(f"Node {port} | {rpc_port}")
        self.port = port
        self.rpc_port = rpc_port
        self.key_file = key_file
        self.process = process

    def to_json_dict(self):
        return {
            "port": self.port,
            "rpc_port": self.rpc_port,
            "bootstrap": self.key_file != None
        }


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


def run_server_in_thread():
    PORT = 8000
    Handler = NodeInfoProvider

    class Server(socketserver.TCPServer):
        allow_reuse_address = True

    httpd = Server(("", PORT), Handler)

    print(f"Server started at http://localhost:{PORT}")

    server_thread = threading.Thread(target=httpd.serve_forever)
    server_thread.daemon = True  # Optionally make the server thread a daemon
    server_thread.start()
    return server_thread, httpd


def main():
    global all_nodes
    server_thread, httpd = run_server_in_thread()
    bootnodes, bootnodes_str = initialize_bootnodes()
    regular_nodes = start_additional_nodes(len(bootnodes), bootnodes_str)
    all_nodes = bootnodes + regular_nodes

    # Wait for all nodes to finish
    for node in all_nodes:
        node.process.wait()

    try:
        # Main thread waits for Ctrl+C
        while server_thread.is_alive():
            time.sleep(1)

    except KeyboardInterrupt:
        print("Ctrl+C pressed. Stopping the server...")
        # Stop the server
        httpd.shutdown()
        httpd.server_close()
        server_thread.join()
        # Run the custom function
        [x.process.kill() for x in all_nodes]


if __name__ == "__main__":
    main()
