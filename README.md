# raft
Raft implementation written in Rust

## How to run
In separate terminals, you can run a manual example of raft's implementation using the command `cargo run <node_name> <port>`.

Provided example nodes so far are:

```Rust
Node {
ip: "127.0.0.1",
port: "5433",
name: "node1",
},
Node {
    ip: "127.0.0.1",
    port: "5434",
    name: "node2",
},
Node {
    ip: "127.0.0.1",
    port: "5435",
    name: "node3",
},
Node {
    ip: "127.0.0.1",
    port: "5436",
    name: "node4",
}
```