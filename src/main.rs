use crate::backend::ConsensusModule;
use crate::messages::NewConnection;
use actix::clock::sleep;
use actix::{Actor, Addr, AsyncContext, Context, System};
use actix_rt::spawn;
use std::env;
use std::time::Duration;
use node_config::{Node, NodesConfig};
use tokio::net::TcpListener;

mod backend;
mod health_connection;
mod messages;
mod node_config;

struct RaftNode {
    node_id: String,
    ip: String,
    port: usize,
    total_nodes: usize,
    address: Option<Addr<ConsensusModule>>,
}

impl RaftNode {
    pub fn new(node_id: String, ip: String, port: usize, total_nodes: usize) -> Self {
        RaftNode {
            node_id,
            ip,
            port,
            total_nodes,
            address: None,
        }
    }

    pub async fn start(&mut self, nodes_config: NodesConfig) {
        let node_id = self.node_id.clone();

        let nodes_config_copy = nodes_config.clone();

        let ctx = Context::<ConsensusModule>::new();
        let mut backend = ConsensusModule::start_connections(self.ip.clone(), self.port, self.node_id.clone(), nodes_config_copy).await;

        let join = spawn(RaftNode::listen_for_connections(node_id, self.ip.clone(), self.port, ctx.address()));
        backend.add_myself(ctx.address());
        backend.add_me_to_connections(ctx.address()).await;

        self.address = Some(ctx.address());

        let nodes_count = nodes_config.nodes.len();
        let last_node = nodes_config.nodes.get(nodes_count - 1).unwrap();
        
        if self.ip == last_node.ip && self.port == last_node.port.parse().unwrap() {
            println!("Running election timer");
            backend.run_election_timer();
        }

        ctx.run(backend);
        join.await.expect("Error in join.await");
    }

    pub async fn listen_for_connections(node_id: String, ip: String, port: usize, ctx_task: Addr<ConsensusModule>) {
        let port = port + 3000;
        let listener = TcpListener::bind(format!("{}:{}", ip, port))
            .await
            .expect("Failed to bind listener");
    
        println!("Node {} is listening on {}:{}", ip, node_id, port);
    
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    println!("Connection accepted from Node: {}", addr);
                    ctx_task
                        .send(NewConnection {
                            id_connection: addr.to_string(),
                            stream,
                        })
                        .await
                        .expect("Error sending new message");
    
                    println!("Connection accepted from Node. Actor created.");
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e);
                }
            }
        }
    }
}

#[actix_rt::main]
async fn main() {

    // cargo run node1 3 5433

    let args: Vec<String> = env::args().collect();

    if args.len() < 4 {
        eprintln!("Usage: {} <node_id> <total_nodes>", args[0]);
        std::process::exit(1);
    }

    let node_id = args[1].to_string();
    let total_nodes: usize = args[2]
        .parse()
        .expect("Invalid total_nodes, must be a number");
    
    let port: usize = args[3]
    .parse()
    .expect("Invalid port, must be a number");

    let mut raft_node = RaftNode::new(node_id, "127.0.0.1".to_string(), port, total_nodes);

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
            }
        ]
    };

    raft_node.start(nodes).await;
}
