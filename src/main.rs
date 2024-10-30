use actix::System;
use raft::{node_config::{Node, NodesConfig}, raft_module::RaftModule};
use utils_lib::{log, set_running_local};
use std::{env, thread};

#[actix_rt::main]
async fn main() {
    set_running_local!();
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        log!("Usage: {} <node_id> <total_nodes>", args[0]);
        std::process::exit(1);
    }

    let node_id = args[1].to_string();
    let port = args[2].to_string();
    let ip: String = "127.0.0.1".to_string();

    // This is for local testing.
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

    thread::spawn( move || {
        println!("Inside thread spawn");
        System::new().block_on(async {
            println!("Inside block on");
            new_raft_instance(node_id, nodes, ip, &port).await;
        });
    });
    println!("End of Main!");
}

async fn new_raft_instance(node_id: String, nodes: NodesConfig, ip: String, original_port: &str) {

    let mut port: usize = original_port.parse().expect("Received an invalid port number");
    port += 2000;

    println!("Starting Raft module");
    // Directly create the RaftModule instance without creating a new runtime
    let mut raft_module = RaftModule::new(node_id.clone(), ip.to_string(), port.clone());

    // Await the start of the Raft module
    raft_module
        .start(nodes, Some(format!("../../../sharding/init_history/init_{}", node_id)))
        .await;

    println!("Raft module started");

    let ctx = raft_module.address.expect("Raft module address is None");
    RaftModule::listen_for_connections(port, ip.to_string(), node_id, ctx);
    println!("Ending New Raft");
}