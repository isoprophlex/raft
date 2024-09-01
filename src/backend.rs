use std::cmp::PartialEq;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use actix::{Actor, ActorContext, Addr, AsyncContext, Context, Handler};
use tokio::net::{TcpListener, TcpStream};
use tokio::task;
use tokio::time::{Instant, sleep};
use crate::backend::State::Follower;
use crate::health_connection::{HealthConnection};
use crate::messages::{AddBackend, AddNode, ConnectionDown, Heartbeat, No, RequestAnswer, RequestedOurVote, StartElection, Vote};
/// Raft RPCs
#[derive(Clone)]
pub enum State {
    Follower,
    Leader,
    Candidate
}
impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (State::Follower, State::Follower) => true,
            (State::Leader, State::Leader) => true,
            (State::Candidate, State::Candidate) => true,
            _ => false,
        }
    }
}
#[derive(Clone)]
pub struct ConsensusModule {
    pub connection_map: Arc<Mutex<HashMap<usize, Addr<HealthConnection>>>>,
    pub node_id: usize,
    pub port: usize,
    pub state: State,
    pub current_term: usize,
    pub election_reset_event: Instant,
    pub votes: Option<u16>,
    pub last_vote: Option<usize>
}
impl Actor for ConsensusModule {
    type Context = Context<Self>;
}

impl ConsensusModule {
    pub async fn start_connections(node_id: usize, total_nodes: usize, port: usize) -> Self {
        let connection_map: Arc<Mutex<HashMap<usize, Addr<HealthConnection>>>> = Arc::new(Mutex::new(HashMap::new()));
        if node_id == 1 {
            let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();
            println!("Node {} is listening on 127.0.0.1:{}", node_id, port);
            let connection_map_clone = Arc::clone(&connection_map);
            let _ = task::spawn_local(async move {
                let mut next_id = node_id + 1;
                // Accept every connection
                while next_id <= total_nodes {
                    if let Ok((stream, _)) = listener.accept().await {
                        let addr = HealthConnection::create_actor(stream, node_id, next_id);
                        connection_map_clone.lock().unwrap().insert(next_id, addr);
                        println!("Connection accepted with ID: {}", next_id);
                        next_id += 1;
                    }
                    if !connection_map_clone.lock().unwrap().is_empty() {
                        let hashmap = connection_map_clone.lock().unwrap();
                        for (id, connection) in hashmap.iter() {
                            for (id_actor, to_send) in hashmap.iter() {
                                if id_actor != id {
                                    connection.send(AddNode {id: id_actor.clone(), node: to_send.clone()}).await.expect("Error sending AddNode");
                                }
                            }
                        }
                    }
                }
            }).await;
        } else if node_id > 1 && node_id < total_nodes {
            for previous_id in 1..node_id {
                let addr = format!("127.0.0.1:{}", previous_id + 8000);
                let stream = TcpStream::connect(addr).await.unwrap();
                println!("Node {} connected to Node {}", node_id, previous_id);

                // Crear actor para la conexión a cada nodo anterior
                let actor_addr = HealthConnection::create_actor(stream, node_id, previous_id);
                connection_map.lock().unwrap().insert(previous_id, actor_addr);
            }

            let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();
            println!("Node {} is listening on 127.0.0.1:{}", node_id, port);

            let connection_map_clone = Arc::clone(&connection_map);
            let _ = task::spawn_local(async move {
                let mut next_id = node_id + 1;
                while next_id <= total_nodes {
                    if let Ok((stream, _)) = listener.accept().await {
                        let addr = HealthConnection::create_actor(stream, node_id, next_id);
                        connection_map_clone.lock().unwrap().insert(next_id, addr);
                        println!("Connection accepted with ID: {}", next_id);
                        next_id += 1;
                    }
                }
                if !connection_map_clone.lock().unwrap().is_empty() {
                    let hashmap = connection_map_clone.lock().unwrap();
                    for (id, connection) in hashmap.iter() {
                        for (id_actor, to_send) in hashmap.iter() {
                            if id_actor != id {
                                connection.do_send(AddNode {id: id_actor.clone(), node: to_send.clone()});
                            }
                        }
                    }
                }
            }).await;
        } else if node_id == total_nodes {
            // Nodo N: Solo se conecta a todos los nodos anteriores
            for previous_id in 1..node_id {
                let addr = format!("127.0.0.1:{}", previous_id + 8000);
                let stream = TcpStream::connect(addr).await.unwrap();
                let actor_addr = HealthConnection::create_actor(stream, node_id, previous_id);
                connection_map.lock().unwrap().insert(previous_id, actor_addr);
            }
            if !connection_map.lock().unwrap().is_empty() {
                let hashmap = connection_map.lock().unwrap();
                for (id, connection) in hashmap.iter() {
                    for (id_actor, to_send) in hashmap.iter() {
                        if id_actor != id {
                            connection.send(AddNode {id: id_actor.clone(), node: to_send.clone()}).await.expect("Error sending AddNode");
                        }
                    }
                }
            }
            // No aceptar conexiones
        }
        Self {
            connection_map,
            node_id,
            port,
            state: Follower,
            election_reset_event: Instant::now(),
            current_term: 0,
            votes: Some(0),
            last_vote: None
        }


    }
    pub fn start_election(&mut self) {
        println!("Node {} starting election at term {}", self.node_id, self.current_term);
        self.state = State::Candidate;
        self.current_term += 1;
        // Vote for myself.
        self.votes = Some(1);
        let save_current_term = self.current_term;
        self.election_reset_event = Instant::now();
        let mut lock = self.connection_map.lock().unwrap();
        let iterator = lock.iter_mut();
        for (_, actor) in iterator {
            actor.try_send(StartElection { id: self.node_id, term: self.current_term }).expect("Error starting election");
        }
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
    pub async fn add_me_to_connections(&self, ctx: Addr<ConsensusModule>) {
        let mut lock = self.connection_map.lock().unwrap();
        let iterator = lock.iter_mut();
        for (_, connection) in iterator {
            connection.send(AddBackend { node: ctx.clone() }).await.expect("Error sending backend to connections");
        }
    }
    pub fn election_timeout(&self) -> Duration {
        Duration::from_millis(150 + rand::random::<u64>() % 150)
    }
    pub fn check_votes(&mut self, _ctx: &mut Context<Self>) {
        let mut actual_votes = self.votes.unwrap();
        actual_votes= self.votes.unwrap() + 1;
        self.votes = Some(actual_votes);
        println!("Votes earned: {}", self.votes.unwrap());
        if self.votes.unwrap() >= (self.connection_map.lock().unwrap().len() as u16 / 2 ) + 1 {
            // Im leader
            self.become_leader(_ctx);
            return;
        }
    }
    pub fn become_leader(&mut self, ctx: &mut Context<Self>) {
        self.state = State::Leader;
        println!("Node {} becomes Leader; term={}", self.node_id, self.current_term);

        ctx.run_interval(Duration::from_millis(1000), |actor, _ctx| {
            let current_term = actor.current_term;

            let connections: Vec<_> = actor.connection_map.lock().unwrap().iter().map(|(id, conn)| (*id, conn.clone())).collect();

            for (id, connection) in connections {
                match connection.try_send(Heartbeat { term: current_term }) {
                    Ok(_) => {}
                    Err(e) => {
                        println!("[ACTOR] Error sending Heartbeat to connection {}: {}", id, e);

                        actor.connection_map.lock().unwrap().remove(&id);
                        for (notifying_id, notifying_actor) in actor.connection_map.lock().unwrap().iter() {
                            let _ = notifying_actor.try_send(ConnectionDown { id });
                        }
                    }
                }
            }

            if actor.state != State::Leader {
                println!("Node {} is no longer the Leader", actor.node_id);
                _ctx.stop();
            }
        });
    }
    pub async fn become_follower(&mut self, new_term: usize) {
        println!("NODE {} became follower at term {}", self.node_id, new_term);
        self.state = State::Follower;
        self.current_term = new_term;
        self.election_reset_event = Instant::now();
        self.run_election_timer();
    }
}
impl Handler<Vote> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: Vote, _ctx: &mut Self::Context) -> Self::Result {
        let vote_term = msg.term;
        println!("Received vote from {} in term {}", msg.id, msg.term);

        if vote_term > self.current_term {
            println!("I'm out of date, now I become a follower.");
            self.state = State::Follower;
            return;
        } else if vote_term == self.current_term {
            self.check_votes(_ctx);
        }
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
impl Handler<ConnectionDown> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: ConnectionDown, _ctx: &mut Self::Context) -> Self::Result {
        println!("Actor {} deleted", msg.id);
        let mut lock = self.connection_map.lock().unwrap();
        lock.remove_entry(&msg.id);
    }
}
impl Handler<RequestedOurVote> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: RequestedOurVote, _ctx: &mut Context<Self>) -> Self::Result {
        let vote_term = msg.term;
        println!("Received vote request from {} in term {}", msg.candidate_id, msg.term);
        if self.last_vote.is_none() {
            println!("First election!");
            self.last_vote = Some(msg.term);
            let lock = self.connection_map.lock().unwrap();
            let actor = lock.get(&msg.candidate_id).unwrap();
            actor.try_send(RequestAnswer {msg: "VOTE".to_string()}).expect("Error sending VOTE HIM to connection");
        }
        else if vote_term > self.current_term {
           println!("I have to vote!");
            let lock = self.connection_map.lock().unwrap();
            let actor = lock.get(&msg.candidate_id).unwrap();
            actor.try_send(RequestAnswer {msg: "VOTE".to_string()}).expect("Error sending VOTE HIM to connection");
        } else if vote_term == self.current_term {
            println!("I have already voted!");
            let lock = self.connection_map.lock().unwrap();
            let actor = lock.get(&msg.candidate_id).unwrap();
            actor.try_send(RequestAnswer {msg: "NO".to_string()}).expect("Error sending VOTE HIM to connection");
        }
    }
}