use crate::backend::ConsensusModule;
use actix::{Addr, AsyncContext, Actor, Context, System};
use std::env;
use std::time::Duration;
use tokio::net::TcpListener;
use actix_rt::spawn;
use actix::clock::sleep;
use crate::messages::NewConnection;

mod backend;
mod health_connection;
mod messages;

struct RaftNode {
    node_id: usize,
    ip: String,
    port: usize,
    total_nodes: usize,
    address: Option<Addr<ConsensusModule>>,
}

impl RaftNode {
    pub fn new(node_id: usize, ip: String, port: usize, total_nodes: usize) -> Self {
        RaftNode {
            node_id,
            ip,
            port,
            total_nodes,
            address: None
        }
    }

    pub async fn start(&mut self) {
    let node_id = self.node_id;
    let port = self.port;

    let ctx = Context::<ConsensusModule>::new();
    let mut backend = ConsensusModule::start_connections(node_id, port).await;

    let join = spawn(listen_for_connections(node_id, port, ctx.address()));
    backend.add_myself(ctx.address());
    backend.add_me_to_connections(ctx.address()).await;
    
    self.address = Some(ctx.address());

    if node_id == self.total_nodes {
        backend.run_election_timer();
    }

    ctx.run(backend);
    join.await.expect("Error in join.await");
    }
}

#[actix_rt::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <node_id> <total_nodes>", args[0]);
        std::process::exit(1);
    }

    let node_id: usize = args[1].parse().expect("Invalid ID, must be a number");
    let total_nodes: usize = args[2]
        .parse()
        .expect("Invalid total_nodes, must be a number");
    let port = node_id + 8000;
    
    let mut raft_node = RaftNode::new(node_id, "127.0.0.1".to_string(), port, total_nodes);
    raft_node.start().await;
}

pub async fn listen_for_connections(node_id: usize, port: usize, ctx_task: Addr<ConsensusModule>) {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to bind listener");

    println!("Node {} is listening on 127.0.0.1:{}", node_id, port);
    let mut new_connection = node_id;

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                new_connection += 1;
                ctx_task
                    .send(NewConnection { id_connection: new_connection, stream })
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