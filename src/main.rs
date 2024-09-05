use std::env;
use actix::{Actor, AsyncContext, Context};
use std::time::Duration;
use tokio::time::sleep;
use crate::backend::ConsensusModule;

mod health_connection;
mod messages;
mod backend;

#[actix_rt::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <node_id> <total_nodes>", args[0]);
        std::process::exit(1);
    }

    let node_id: usize = args[1].parse().expect("Invalid ID, must be a number");
    let total_nodes: usize = args[2].parse().expect("Invalid total_nodes, must be a number");
    let port = node_id + 8000;

    let ctx = Context::<ConsensusModule>::new();
    // First node only accepts
   let mut backend = ConsensusModule::start_connections(node_id, total_nodes, port).await;
    backend.add_myself(ctx.address());
    backend.add_me_to_connections(ctx.address()).await;
    if node_id == total_nodes {
        backend.run_election_timer();
    }
    ctx.run(backend);
    sleep(Duration::from_secs(15)).await;
}