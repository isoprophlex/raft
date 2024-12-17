use raft::{
    node_config::{Node, NodesConfig},
    raft_module::RaftModule,
};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::{env, thread};
use utils_lib::{log, set_running_local};

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
    let (raft_transmitter, external_receiver): (Sender<bool>, Receiver<bool>) = mpsc::channel();
    let (external_transmitter, raft_receiver): (Sender<bool>, Receiver<bool>) = mpsc::channel();
    
    // TODO this is for local testing. Delete this
    let nodes = NodesConfig {
        nodes: vec![
            Node {
                ip: "127.0.0.1".to_string(),
                port: "3333".to_string(),
                name: "node1".to_string(),
            },
            Node {
                ip: "127.0.0.1".to_string(),
                port: "3334".to_string(),
                name: "node2".to_string(),
            },
            Node {
                ip: "127.0.0.1".to_string(),
                port: "3335".to_string(),
                name: "node3".to_string(),
            },
            Node {
                ip: "127.0.0.1".to_string(),
                port: "3336".to_string(),
                name: "node4".to_string(),
            },
        ],
    };
    thread::spawn(move || loop {
        match external_receiver.recv() {
            Ok(message) => {
                log!("[MAIN] raft is leader: {:?}", message);
                external_transmitter.send(true).expect("Failed to send message");
            }
            Err(_) => {
                log!("[MAIN] Receiver channel closed.");
                break;
            }
        }
    });
    
    raft_node.start(nodes, None, raft_transmitter, raft_receiver, false).await;
}
