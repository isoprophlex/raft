use std::collections::HashMap;
use std::time::Duration;
use actix::{Actor, ActorContext, Addr, AsyncContext, Context, Handler};
use tokio::net::{TcpListener, TcpStream};
use tokio::task;
use tokio::time::{Instant, sleep};
use crate::backend::State::{Candidate, Follower};
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
    pub connection_map: HashMap<usize, Addr<HealthConnection>>,
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
    pub fn start_election(&mut self) {
        self.state = Candidate;
        self.current_term = self.current_term + 1;
        println!("NEW CURRENT TERM: {}", self.current_term);
        self.election_reset_event = Instant::now();
        for (_, connection) in &self.connection_map {
            connection.try_send(StartElection { id: self.node_id, term: self.current_term}).unwrap()
        }
    }
    pub fn election_timeout(&self) -> Duration {
        Duration::from_millis(150 + rand::random::<u64>() % 150)
    }
    pub fn run_election_timer(&mut self) {
        let timeout_duration = self.election_timeout();
        let term_started = self.current_term;
        println!("[NODE {}] ELECTION TIMER STARTED {}, TERM: {}", self.node_id, timeout_duration.as_millis(), term_started);
        loop {
            if self.state != State::Candidate && self.state != Follower {
                println!("[NODE {}] I'M THE CURRENT LEADER", self.node_id);
                return;
            }
            if term_started != self.current_term {
                println!("[NODE {}] IN ELECTION TIMER TERM CHANGED FROM {} TO {}", self.node_id, term_started, self.current_term);
                return;
            }
            let elapsed = Instant::now().duration_since(self.election_reset_event);
            println!("ELAPSED: {}", elapsed.as_millis());
            if elapsed >= timeout_duration {
                self.start_election();
                return;
            }
        }
    }
    pub fn add_myself(&mut self, myself: Addr<ConsensusModule>) {
        self.myself = Option::from(myself);
    }
    pub async fn add_me_to_connections(&self, ctx: Addr<ConsensusModule>) {
        for (_, connection) in &self.connection_map {
            connection.send(AddBackend { node: ctx.to_owned() }).await.expect("Error sending backend to connections");
        }
    }
    pub fn check_votes(&mut self, _ctx: &mut Context<Self>, vote_term: u16) {
        let mut actual_votes = self.votes.unwrap();
        println!("CURRENT TERM ON CHECK VOTEs: {}", self.current_term);
        println!("I GOT A VOTE ON TERM: {}", vote_term);
        if vote_term > self.current_term as u16 {
            println!("I'm out of date, now I become a follower.");
            self.state = Follower;
            self.votes = Some(0);
            return;
        } else if vote_term as usize == self.current_term {
            actual_votes= self.votes.unwrap() + 1;
            self.votes = Some(actual_votes);
            println!("Votes earned: {}", self.votes.unwrap());

        }
        if self.votes.unwrap() >= (self.connection_map.len() as u16 / 2 ) + 1 {
            // Im leader
            self.become_leader(_ctx);
            return;
        }
    }
    pub fn become_leader(&mut self, ctx: &mut Context<Self>) {
        self.state = State::Leader;
        println!("Node {} becomes Leader; term={}", self.node_id, self.current_term);
        let current_term = self.current_term;
        let node_id = self.node_id;
        let mut connection_map = self.connection_map.clone();
        let mut connection_map2 = self.connection_map.clone();

        ctx.run_interval(Duration::from_millis(1000), move |actor, _ctx| {
            let mut ids_to_delete: Vec<usize> = Vec::new();
            for (id, connection) in &mut connection_map {
                match connection.try_send(Heartbeat { term: current_term }) {
                    Ok(_) => {}
                    Err(e) => {
                        println!("[ACTOR] Error sending Heartbeat to connection {}: {}", id, e);
                        for (_, notifying_actor) in &connection_map2 {
                            let _ = notifying_actor.do_send(ConnectionDown { id: *id });
                        }
                        ids_to_delete.push(*id);
                    }
                }
            }
            if !ids_to_delete.is_empty() {
                for id in ids_to_delete {
                    connection_map.remove(&id);
                }
            }
            if actor.state != State::Leader {
                println!("Node {} is no longer the Leader", node_id);
                _ctx.stop();
            }
        });
    }

}

impl Handler<AddNode> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: AddNode, _ctx: &mut Self::Context) -> Self::Result {
        self.connection_map.insert(msg.id, msg.node);
        println!("Node {} added to connections", msg.id);
    }
}
impl Handler<RequestedOurVote> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: RequestedOurVote, _ctx: &mut Context<Self>) -> Self::Result {
        let vote_term = msg.term;
        println!("Received vote request from {} in term {}", msg.candidate_id, msg.term);
        if self.last_vote.is_none() || self.last_vote == Some(0) {
            println!("First election!");
            self.last_vote = Some(msg.term);
            let lock = self.connection_map.clone();
            let actor = lock.get(&msg.candidate_id).unwrap();
            actor.try_send(RequestAnswer {msg: "VOTE".to_string(), term: vote_term}).expect("Error sending VOTE HIM to connection");
        }
        else if vote_term > self.current_term {
            println!("I have to vote!");
            let lock = self.connection_map.clone();
            let actor = lock.get(&msg.candidate_id).unwrap();
            actor.try_send(RequestAnswer {msg: "VOTE".to_string(), term: vote_term}).expect("Error sending VOTE HIM to connection");
        } else if vote_term == self.current_term {
            println!("I have already voted!");
            let lock = self.connection_map.clone();
            let actor = lock.get(&msg.candidate_id).unwrap();
            actor.try_send(RequestAnswer {msg: "NO".to_string(), term: self.current_term}).expect("Error sending VOTE HIM to connection");
        }
    }
}
impl Handler<Vote> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: Vote, _ctx: &mut Self::Context) -> Self::Result {
        let vote_term = msg.term;
        println!("Received vote from {} in term {}", msg.id, msg.term);
        println!("My current term is {}", self.current_term);
        self.check_votes(_ctx, msg.term as u16);
    }
}
impl Handler<No> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: No, _ctx: &mut Self::Context) -> Self::Result {
        let vote_term = msg.term;
        println!("Received NO in term {}", msg.term);

        if vote_term > self.current_term as u16 {
            println!("I'm out of date, now I become a follower.");
            self.state = State::Follower;
            return;
        }
    }
}