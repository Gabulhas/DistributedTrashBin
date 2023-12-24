import requests
from start_nodes import Node
from sys import argv
from time import sleep
from typing import List
import threading
import json


def get_all_nodes():
    url = f"http://0.0.0.0:8000"
    res = requests.get(url)
    return res.json()


def add_new_value(node_port, key, value):
    url = f"http://0.0.0.0:{node_port}/add-key"
    headers = {"Content-Type": "application/json"}
    data = {"key": key, "value": list(value.encode())}
    try:
        response = requests.post(url, headers=headers, data=json.dumps(data))
        return response.json()
    except Exception as e:
        return {"error": str(response.content), "url": url, "info": data}


def get_value(node_port, key):
    url = f"http://0.0.0.0:{node_port}/value/{key}"
    try:
        response = requests.get(url)
        return response.json()
    except Exception as e:
        return {"error": str(response.content), "url": url}


def wait(node_number, node_port, key):
    while True:
        res = get_value(node_port, key)
        print(node_number, ">", res)
        if "error" in res:
            return
        if "Owner" in res["value"].keys():
            return

        sleep(1)


def background(func, *args, **kwargs):
    thread = threading.Thread(target=func, args=args, kwargs=kwargs)
    thread.daemon = True  # Daemonize the thread
    thread.start()


def interpreter(program: List[str], nodes):

    for line in program:
        if line == "" or line.startswith("--") or line.startswith("\n"):
            continue
        print(">", line)
        tokens = line.split()
        command = tokens[0]
        key = " ".join(tokens[2:])

        if command == "allbgwait":
            key = " ".join(tokens[1:])
            [background(wait, i+1, node["rpc_port"], key)
             for i, node in enumerate(nodes)]
            continue
        node_port = nodes[int(tokens[1]) - 1]["rpc_port"]
        if command == "new":
            separator = tokens.index("|")
            key = " ".join(tokens[2:separator])
            value = " ".join(tokens[separator+1:])
            add_new_value(node_port, key, value)
        elif command == "get":
            get_value(node_port, key)
        elif command == "wait":
            wait(int(tokens[1]), node_port, key)
        elif command == "bgwait":
            background(wait, int(tokens[1]), node_port, key)
        elif command == "sleep":
            print(f"Sleeping for {int(tokens[1])}")
            sleep(int(tokens[1]))


def read_program(path):
    f = open(path)
    lines = f.readlines()
    f.close()
    return lines


def main():
    program = read_program(argv[1])
    nodes = get_all_nodes()
    print(f"Got {len(nodes)} nodes")
    interpreter(program, nodes)


if __name__ == "__main__":
    main()
