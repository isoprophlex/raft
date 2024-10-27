use raft::{node_config::{Node, NodesConfig}, raft_module::RaftModule};
use utils_lib::{log, set_running_local};
use std::env;

#[actix_rt::main]
async fn main() {
    set_running_local!();
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        log!("Usage: {} <node_id> <total_nodes>", args[0]);
        std::process::exit(1);
    }

    let node_id = args[1].to_string();

    let port: usize = args[2].parse().expect("Invalid port, must be a number");

    let mut raft_node = RaftModule::new(node_id, "127.0.0.1".to_string(), port);

    // TODO this is for local testing. Delete this
    let nodes = NodesConfig {
        nodes: vec![
            Node {
                ip: "127.0.0.1".to_string(),
                port: "5433".to_string(),
                name: "node1".to_string(),
            },
            Node {
                ip: "127.0.0.1".to_string(),
                port: "5434".to_string(),
                name: "node2".to_string(),
            },
            Node {
                ip: "127.0.0.1".to_string(),
                port: "5435".to_string(),
                name: "node3".to_string(),
            },
            Node {
                ip: "127.0.0.1".to_string(),
                port: "5436".to_string(),
                name: "node4".to_string(),
            },
        ],
    };

    raft_node.start(nodes, None).await;
}
