use std::env;
use actix::{Addr};
use std::collections::HashMap;
use tokio::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task;
use tokio::time::sleep;
use crate::backend::Backend;
use crate::health_connection::{HealthConnection};
use crate::messages::{AddNode, Coordinator};

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

    // First node only accepts
   let mut backend = Backend::start_connections(node_id, total_nodes, port).await;
    if node_id == total_nodes {
        backend.run_election_timer().await;
    }
    sleep(Duration::from_secs(15)).await;
}