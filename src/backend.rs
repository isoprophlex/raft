use std::collections::HashMap;
use std::time::Duration;
use actix::{Actor, Addr, AsyncContext, Context, Handler};
use tokio::net::{TcpListener, TcpStream};
use tokio::task;
use tokio::time::{Instant, sleep};
use crate::backend::State::Follower;
use crate::health_connection::HealthConnection;
use crate::messages::{AddBackend, AddNode, ConnectionDown, Heartbeat, No, RequestAnswer, RequestedOurVote, StartElection, UpdateTerm, Vote};

/// Raft RPCs
#[derive(Clone)]
pub enum State {
    Follower,
    Leader,
    Candidate,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        matches!((self, other),
            (State::Follower, State::Follower) |
            (State::Leader, State::Leader) |
            (State::Candidate, State::Candidate)
        )
    }
}

#[derive(Clone)]
pub struct ConsensusModule {
    pub connection_map: HashMap<usize, Addr<HealthConnection>>, // Sin Arc<Mutex<>>
    pub node_id: usize,
    pub port: usize,
    pub state: State,
    pub current_term: usize,
    pub election_reset_event: Instant,
    pub votes: Option<u16>,
    pub last_vote: Option<usize>,
    myself: Option<Addr<ConsensusModule>>,
}

impl Actor for ConsensusModule {
    type Context = Context<Self>;
}

impl ConsensusModule {
    pub async fn start_connections(node_id: usize, total_nodes: usize, port: usize) -> Self {
        let mut connection_map = HashMap::new();

        if node_id == 1 {
            let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();
            println!("Node {} is listening on 127.0.0.1:{}", node_id, port);

            let mut next_id = node_id + 1;
            while next_id <= total_nodes {
                if let Ok((stream, _)) = listener.accept().await {
                    let addr = HealthConnection::create_actor(stream, node_id, next_id);
                    connection_map.insert(next_id, addr);
                    println!("Connection accepted with ID: {}", next_id);
                    next_id += 1;
                }
            }

        } else if node_id > 1 && node_id < total_nodes {
            for previous_id in 1..node_id {
                let addr = format!("127.0.0.1:{}", previous_id + 8000);
                let stream = TcpStream::connect(addr).await.unwrap();
                println!("Node {} connected to Node {}", node_id, previous_id);

                let actor_addr = HealthConnection::create_actor(stream, node_id, previous_id);
                connection_map.insert(previous_id, actor_addr);
            }

            let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();
            println!("Node {} is listening on 127.0.0.1:{}", node_id, port);

            let mut next_id = node_id + 1;
            while next_id <= total_nodes {
                if let Ok((stream, _)) = listener.accept().await {
                    let addr = HealthConnection::create_actor(stream, node_id, next_id);
                    connection_map.insert(next_id, addr);
                    println!("Connection accepted with ID: {}", next_id);
                    next_id += 1;
                }
            }

        } else if node_id == total_nodes {
            for previous_id in 1..node_id {
                let addr = format!("127.0.0.1:{}", previous_id + 8000);
                let stream = TcpStream::connect(addr).await.unwrap();
                let actor_addr = HealthConnection::create_actor(stream, node_id, previous_id);
                connection_map.insert(previous_id, actor_addr);
            }
        }
        Self {
            connection_map,
            node_id,
            port,
            state: Follower,
            election_reset_event: Instant::now(),
            current_term: 0,
            votes: Some(0),
            last_vote: None,
            myself: None,
        }
    }



}

impl Handler<AddNode> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: AddNode, _ctx: &mut Self::Context) -> Self::Result {
        self.connection_map.insert(msg.id, msg.node);
        println!("Node {} added to connections", msg.id);
    }
}
