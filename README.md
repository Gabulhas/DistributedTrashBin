# DistributedTrashBin

Welcome to `DistributedTrashBin`, a Rust-based (btw) exploration into the world of distributed directory databases, where we dive into the complexities of networked data storage with a hint of humor and a lot of learning.

[Check out this](https://jamusti.co/making-a-distributed-directory-database/), where I explain in detail what I want to do.


## Overview

`DistributedTrashBin` is not your ordinary database project. It's a playful yet serious foray into the creation of a distributed directory database using Rust. This project aims to implement a networked system for storing and retrieving data across multiple nodes, ensuring data integrity and accessibility in a distributed environment.

## Technical Details

At its core, `DistributedTrashBin` leverages the power and safety of Rust, along with the `libp2p` framework, to establish peer-to-peer connections between nodes in the network. Here's a sneak peek into the technical side:

- **Distributed System**: Utilizes the principles of distributed computing to ensure data is spread across multiple nodes for redundancy and reliability.

- **Rust Programming**: Implements the system in Rust, known for its performance and safety, especially in concurrent environments.

- **`libp2p` Framework**: Harnesses the `libp2p` library for establishing decentralized, peer-to-peer network connections, crucial for the distributed nature of the database.

- **Dynamic Data Handling**: Manages a directory structure that allows nodes to efficiently store, retrieve, and update data.

- **Self-Healing Network**: Designs a system capable of handling node failures and network partitions, maintaining data integrity and availability.


# The "Why Am I Even Bothering?" Section
Alright, let's get this over with. DistributedTrashBin – because why make something useful when you can spend hours crafting a distributed system that might just end up being a glorified digital paperweight?

## The Gist of This thing

`DistributedTrashBin` is based on the principles of the Arrow Distributed Directory Protocol (ADDP), but with my own twist and a dash of reckless ambition. It's about creating a distributed network where data isn't just dumped but is intelligently managed across multiple nodes.


## The Technical Nitty-Gritty
ADDP and Its Cousins: ADDP is all about managing how data (or "objects") is accessed and transferred across a network.
Think of it as a game of hot potato, but with data, and slightly more complex rules.
Basically, in order to access an object, a node has to make use of a directory (basically just a graph made of multiple links between nodes) to send requests.

## Think of this as some sort of story like:
Imagine you are the only student in your school that has the new GameBoy, but (Tom) asks for it.

You borrowed your GameBoy to Tom. Tom borrowed it to his sister. His sister borrowed it to her friend Mary, etc.

You don't currently know where your GameBoy is, but if you ask Tom, and Tom asks his sister and so on and on, they can make your request reach.
So this is some sort of (linked) list.

Now imagine that Alice, Craig and Ben ask the current borrower, Bob, for the GameBoy, and he decided to Borrow it to Alice (because he likes her). Now Craig and Ben will have to ask Bob where the GameBoy went.
This way, the borrowing scheme (idk a better name) has become a graph (to be more specific, a tree).

(Are you lost already?)

So Ben's and Craig's requests will go through Bob, to reach Alice.
If Alice decided to borrow it to Ben, then, Craig's request path will be Bob -> Alice -> Ben.


Now, if you want to get your GameBoy back, your request path will be Tom -> His sister -> Mary -> ... -> Bob -> Alice -> Ben.

And yet, you still have to consider that other people might want to try that new GameBoy as well, so even though that it can reach Ben, Ben can decide to lend it to someone.


## What I'm trying to do

A more technical explanation can be found in the article, but that's the basic idea.
A Node (a person, or a machine), doesn't have to know where the object is, just has to know who to ask.

Using this kind of system, two distributed data structures are formed:
- A Queue. Basically the order that each person will get to play the GameBoy.
- A Tree (Directory). A distributed graph made of people knowing who to ask

If someone is waiting for the GameBoy, and a request reaches them, after that person is done, it will borrow the GameBoy to the author of the request.

This way, we can guarantee multiple benefits, some of them being:
- Mutual exclusion. Only one person at a time plays with the GameBoy (has access to the object)
- No hot node. There's no one that gets all the requests and gets annoyed.


Now think of this as a directory (not necessary like your filesystem), that instead of just being shared a single GameBoy, multiple toys are shared and not necessarily with the same scheme/graph, as not everyone want to play with the same things.

That's what we trying to achieve this.
A Database/Node system, where
- Databases are of the Key Value Type.
- Alongside a Key, Nodes either store a value, or a pointer to another node, and the set of these pointers form the multiple directories.


# Extra

This isn't easy to grasp in one go. For that, during my bachelor's I've created an implementation and a (live) visualization tool of these mechanisms. You can check it [Here](https://github.com/Gabulhas/Arrow-Distributed-Directory-Protocol). It includes some videos and you can run it on your machine, aswell as interact with the network

## Contributing

Feel free to dive into the `DistributedTrashBin`!
I'm joking, I can't be f***ed to manage this project.
[INSERT CLICHÉ CONTRIBUTING THING HERE]

## License

This project is licensed under a license. You have to guess which one ;)
