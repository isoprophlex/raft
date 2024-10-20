use crate::backend::State::{Candidate, Follower};
use crate::health_connection::HealthConnection;
use crate::messages::{Ack, AddBackend, AddNode, AskIfLeader, ConnectionDown, Coordinator, Heartbeat, NewConnection, NewLeader, No, Reconnection, RequestAnswer, RequestedOurVote, StartElection, UpdateID, Vote, HB, ID};
use crate::node_config::NodesConfig;
use actix::{Actor, ActorContext, Addr, AsyncContext, Context, Handler, SpawnHandle};
use std::collections::HashMap;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::Instant;
use inline_colorization::*;
use std::fs::File;
use std::io::{Write, Read};
use chrono::{DateTime, Utc};
use std::time::SystemTime;

/// Raft RPCs
#[derive(Clone)]
pub enum State {
    Follower,
    Leader,
    Candidate,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (State::Follower, State::Follower)
                | (State::Leader, State::Leader)
                | (State::Candidate, State::Candidate)
        )
    }
}

#[derive(Clone)]
pub struct ConsensusModule {
    pub connection_map: HashMap<String, Addr<HealthConnection>>,
    pub node_id: String,
    pub ip: String,
    pub port: usize,
    pub state: State,
    pub current_term: usize,
    pub election_reset_event: Instant,
    pub votes: Option<u16>,
    pub last_vote: Option<usize>,
    pub leader_id: Option<String>,
    myself: Option<Addr<ConsensusModule>>,
    pub heartbeat_handle: Option<SpawnHandle>,
    pub heartbeat_check_handle: Option<SpawnHandle>,
}

impl Actor for ConsensusModule {
    type Context = Context<Self>;
}

impl ConsensusModule {
    /// Checks if this is the first time running the node.
    ///
    /// # Arguments
    /// * `file_path` - The path to the file where the node's run history is stored.
    ///
    /// # Returns
    /// `true` if this is the first time running the node, otherwise `false`.
    fn is_first_time_running(file_path: &str) -> bool {
        let mut first_run = false;
        // Node was previously initialized, not the first run
        if let Ok(mut file) = File::open(file_path) {
            let mut timestamp = String::new();
            match file.read_to_string(&mut timestamp) {
                Ok(_) => {
                    println!("\nNode already ran in the past. Last run: {}", timestamp);
                }
                Err(e) => {
                    eprintln!("Error reading timestamp: {}", e);
                }
            }
        } else {
            println!("\nFirst time running the node");
            first_run = true;
        }

        Self::update_timestamp(file_path);

        first_run
    }

    /// Updates the timestamp of the node's last run.
    ///
    /// # Arguments
    /// * `file_path` - The path to the file where the timestamp will be written.
    fn update_timestamp(file_path: &str) {
        let mut file = File::create(file_path).expect("Failed to create initialization file");

        // Convert SystemTime to DateTime<Utc> directly
        let current_time = SystemTime::now();
        let datetime: DateTime<Utc> = current_time.into();
        let formatted_time = datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string();

        file.write_all(formatted_time.as_bytes()).expect("Failed to write timestamp");
    }

    /// Starts connections between nodes based on `node_id` and `total_nodes`.
    ///
    /// # Arguments
    /// * `self_ip` - The IP address of the current node.
    /// * `self_port` - The port on which the current node will listen for connections.
    /// * `self_id` - The identifier of the current node.
    /// * `nodes_config` - The configuration containing all nodes in the network.
    ///
    /// # Returns
    /// A new consensus module with the connection map initialized.
    pub async fn start_connections(self_ip: String, self_port: usize, self_id: String, nodes_config: NodesConfig) -> Self {
        let mut connection_map = HashMap::new();
        let self_port = self_port + 3000;

        let file_path = format!("init_history/init_{}.txt", self_id);
        let first_run = Self::is_first_time_running(&file_path);

        for node in nodes_config.nodes {
            println!("\n{color_green}>>> Node name: {:?}, ip: {}, port: {}{style_reset}", node.name, node.ip, node.port);

            let node_ip = node.ip;
            let node_port = match node.port.parse::<usize>() {
                Ok(port) => port + 3000,
                Err(e) => {
                    println!("Error parsing port for node {}: {}", node.name, e);
                    continue;
                }
            };
            let node_id = node.name;

            // Si el nodo es el mismo que el actual, se detienen las conexiones si es la primera vez
            if node_ip == self_ip && node_port == self_port {
                println!("{color_magenta}Is self.{style_reset}");
                if first_run {
                    break;
                }
                continue;
            }

            let addr = format!("{}:{}", node_ip, node_port);
            let stream = match TcpStream::connect(addr).await {
                Ok(stream) => stream,
                Err(e) => {
                    println!("{color_magenta}Could not connect to Node {}: {}{style_reset}", node_id, e);
                    continue;
                }
            };
            println!("{color_green}Node {} connected to Node {}{style_reset}", self_id, node_id);

            let actor_addr = HealthConnection::create_actor(stream, node_id.clone());
            if let Err(e) = actor_addr.try_send(ID {
                ip: self_ip.clone(),
                port: self_port,
                id: self_id.clone(),
                just_arrived: true
            }) {
                println!("Error sending ID to node {}: {}", node_id, e);
                continue;
            }
            connection_map.insert(node_id.clone(), actor_addr);
        }

        Self {
            connection_map,
            node_id: self_id,
            ip: self_ip,
            port: self_port,
            state: Follower,
            election_reset_event: Instant::now(),
            current_term: 0,
            votes: Some(0),
            last_vote: None,
            myself: None,
            leader_id: None,
            heartbeat_handle: None,
            heartbeat_check_handle: None,
        }
    }

    /// Increments the current term and sends `StartElection` messages to connected nodes.
    pub fn start_election(&mut self) {
        self.state = Candidate;
        self.current_term += 1;
        println!("{color_yellow}[START ELECTION] New current term: {}{style_reset}", self.current_term);

        self.election_reset_event = Instant::now();
        let connection_clone = self.connection_map.clone();
        if connection_clone.len() < 1 {
            self.state = State::Leader;
        }
        for (id, connection) in connection_clone {
            match connection.try_send(StartElection {
                id: self.node_id.clone(),
                term: self.current_term,
            }) {
                Ok(_) => {}
                Err(e) => {
                    println!("Error sending StartElection to Node {}: {}", id, e);
                    self.connection_map.remove(&id);
                }
            }
        }
    }

    /// Calculates and returns the timeout duration for elections.
    ///
    /// # Returns
    /// A `Duration` representing a random timeout value for the election.
    pub fn election_timeout(&self) -> Duration {
        Duration::from_millis(1000 + rand::random::<u64>() % 150)
    }

    /// Starts the election timer and checks the elapsed time to determine if a new election should be initiated.
    /// The last node always calls elections, even when reconnecting.
    pub fn run_election_timer(&mut self) {
        let timeout_duration = self.election_timeout();
        let term_started = self.current_term;
        println!(
            "[TIMER] ELECTION TIMER STARTED {}, TERM: {}",
            timeout_duration.as_millis(),
            term_started
        );
        loop {
            // If it's the leader
            if self.state == State::Leader {
                return;
            }

            // If it's NOT the leader
            if term_started != self.current_term {
                return;
            }

            let elapsed = Instant::now().duration_since(self.election_reset_event);
            if elapsed >= timeout_duration {
                self.start_election();
                return;
            }
        }
    }
    /// Adds the current node's address to its own connection list.
    ///
    /// # Arguments
    /// * `myself` - The address of the current consensus module.
    pub fn add_myself(&mut self, myself: Addr<ConsensusModule>) {
        self.myself = Option::from(myself);
    }
    /// Adds the current node to the connections of all other nodes.
    ///
    /// # Arguments
    /// * `ctx` - The address of the current consensus module to be added.
    pub async fn add_me_to_connections(&self, ctx: Addr<ConsensusModule>) {
        for connection in self.connection_map.values() {
            connection
                .send(AddBackend {
                    node: ctx.clone(),
                })
                .await
                .expect("Error sending backend to connections");
        }
    }
    /// Checks the votes received and updates the state accordingly.
    ///
    /// # Arguments
    /// * `vote_term` - The term for which the votes are being checked.
    pub fn check_votes(&mut self, _ctx: &mut Context<Self>, vote_term: u16) {
        if vote_term > self.current_term as u16 {
            self.state = Follower;
            self.votes = Some(0);
            return;
        } else if vote_term as usize == self.current_term {
            if let Some(votes) = self.votes {
                self.votes = Some(votes + 1);
            } else {
                eprintln!("Error: `votes` is None when trying to increment.");
                return;
            }
        }
        if let Some(votes) = self.votes {
            if votes >= (self.connection_map.len() as u16 / 2) + 1 && self.state != State::Leader {
                self.become_leader(_ctx);
            }
        } else {
            eprintln!("Error: `votes` is None when checking for majority.");
        }
    }


    /// Marks the node as the leader and begins sending heartbeat messages to other nodes.
    ///
    /// # Arguments
    /// * `ctx` - The Actix context needed to manage asynchronous execution.
    pub fn become_leader(&mut self, ctx: &mut Context<Self>) {
        self.state = State::Leader;
        self.leader_id = Some(self.node_id.clone());
        println!(
            "{color_blue}[NODE {}] I'm the new leader, term: {}{style_reset}", self.node_id, self.current_term
        );
        self.announce_leader(ctx);

        if let Some(handle) = self.heartbeat_handle.take() {
            ctx.cancel_future(handle);
        }

        let current_term = self.current_term;
        let node_id = self.node_id.clone();

        let handle = ctx.run_interval(Duration::from_millis(1000), move |actor, ctx| {
            let mut ids_to_delete: Vec<String> = Vec::new();

            for (id, connection) in &mut actor.connection_map {
                println!("{color_red}[❤️] Sending Heartbeat to connection {} {style_reset}", id);
                match connection.try_send(Heartbeat { term: current_term }) {
                    Ok(_) => {}
                    Err(e) => {
                        println!(
                            "{color_red}[❤️] Error sending Heartbeat to connection {}: {}{style_reset}",
                            id, e
                        );
                        ids_to_delete.push(id.clone());
                    }
                }
            }

            for id in &ids_to_delete {
                for notifying_actor in actor.connection_map.values() {
                    notifying_actor.do_send(ConnectionDown { id: id.clone() });
                }
            }

            if !ids_to_delete.is_empty() {
                for id in ids_to_delete {
                    actor.connection_map.remove(&id);
                }
            }

            if actor.state != State::Leader {
                println!("{color_blue}Node {} is no longer the Leader{style_reset}", node_id);
                ctx.stop();
            }
        });

        self.heartbeat_handle = Some(handle);
    }


    /// Marks the node as the leader and begins sending heartbeat messages to other nodes.
    ///
    /// # Arguments
    /// * `ctx` - The Actix context needed to manage asynchronous execution.
    pub fn announce_leader(&mut self, _ctx: &mut Context<Self>) {
        for actor in self.connection_map.values() {
            if let Err(e) = actor.try_send(Coordinator {
                term: self.current_term,
                id: self.node_id.clone(),
            }) {
                eprintln!("Error sending Coordinator message to actor: {}", e);
            }
        }
    }

    /// Starts an interval to check for heartbeat reception.
    ///
    /// If no heartbeat is received within the specified interval, a new election is initiated.
    ///
    /// # Arguments
    /// * `ctx` - The Actix context for the actor's execution.
    pub fn start_heartbeat_check(&mut self, ctx: &mut Context<Self>) {
        if let Some(handle) = self.heartbeat_check_handle.take() {
            ctx.cancel_future(handle);
        }

        let timeout_duration = self.election_timeout();
        let node_id = self.node_id.clone();

        let handle = ctx.run_interval(timeout_duration, move |actor, _ctx| {
            let elapsed = Instant::now().duration_since(actor.election_reset_event);

            if actor.state == State::Leader {
                println!(
                    "{color_blue}[NODE {}] I'm the new leader{style_reset}\n. Stopping heartbeat check.", node_id
                );
                if let Some(check_handle) = actor.heartbeat_check_handle.take() {
                    _ctx.cancel_future(check_handle);
                }
                actor.heartbeat_check_handle = None;
                return;
            }

            if elapsed >= timeout_duration {
                println!(
                    "{color_red}[❤️] No heartbeat received, starting election{style_reset}",
                );
                actor.start_election();
            }
        });

        self.heartbeat_check_handle = Some(handle);
    }

}

impl Handler<AddNode> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: AddNode, _ctx: &mut Self::Context) -> Self::Result {
        self.connection_map.insert(msg.id.clone(), msg.node);
    }
}
impl Handler<RequestedOurVote> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: RequestedOurVote, _ctx: &mut Context<Self>) -> Self::Result {
        let vote_term = msg.term;

        if msg.term < self.current_term {
            if let Some(connection) = self.connection_map.get(&msg.candidate_id) {
                if let Some(leader_id) = &self.leader_id {
                    if let Err(e) = connection.try_send(NewLeader {
                        id: leader_id.clone(),
                        term: self.current_term,
                    }) {
                        eprintln!("Error sending NewLeader to candidate: {}", e);
                    }
                }
            }
            return;
        }

        if self.last_vote.is_none() || self.last_vote == Some(0) {
            self.last_vote = Some(msg.term);
            if let Some(actor) = self.connection_map.get(&msg.candidate_id) {
                if let Err(e) = actor.try_send(RequestAnswer {
                    msg: "VOTE".to_string(),
                    term: vote_term,
                }) {
                    eprintln!("Error sending VOTE HIM to connection: {}", e);
                }
            }
        } else if vote_term > self.current_term {
            if let Some(actor) = self.connection_map.get(&msg.candidate_id) {
                if let Err(e) = actor.try_send(RequestAnswer {
                    msg: "VOTE".to_string(),
                    term: vote_term,
                }) {
                    eprintln!("Error sending VOTE HIM to connection: {}", e);
                }
            }
        } else if vote_term == self.current_term {
            if let Some(actor) = self.connection_map.get(&msg.candidate_id) {
                if let Err(e) = actor.try_send(RequestAnswer {
                    msg: "NO".to_string(),
                    term: self.current_term,
                }) {
                    eprintln!("Error sending NO VOTE to connection: {}", e);
                }
            }
        }
    }

}
impl Handler<Vote> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: Vote, _ctx: &mut Self::Context) -> Self::Result {
        println!("{color_bright_yellow}[VOTE] Received vote from {} in term {}{style_reset}", msg.id, msg.term);
        self.check_votes(_ctx, msg.term as u16);
    }
}
impl Handler<ConnectionDown> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: ConnectionDown, _ctx: &mut Self::Context) -> Self::Result {
        println!("Connection with {} lost", msg.id);
        self.connection_map.remove_entry(&msg.id);
    }
}
impl Handler<NewLeader> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: NewLeader, _ctx: &mut Self::Context) -> Self::Result {
        self.leader_id = Some(msg.id.clone());
        if self.leader_id != Some(self.node_id.clone()) {
            self.state = Follower;
        }
        if self.current_term < msg.term {
            self.current_term = msg.term;
        }
        println!("{color_bright_blue} New leader is {}", msg.id);
        self.start_heartbeat_check(_ctx);
    }
}
impl Handler<HB> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: HB, _ctx: &mut Self::Context) -> Self::Result {
        if self.current_term < msg.term as usize {
            println!("{color_red}[❤️] I was out of date, updating to {}{style_reset}", msg.term);
            self.current_term = msg.term as usize;
            return;
        }

        self.election_reset_event = Instant::now();

        let leader_id = match &self.leader_id {
            Some(id) => id,
            None => {
                println!("{color_red}[❤️] I don't have a leader, ignoring heartbeat{style_reset}");
                return;
            }
        };

        let leader_actor = match self.connection_map.get(leader_id) {
            Some(actor) => actor,
            None => {
                println!("{color_red}[❤️] Leader not found in connection map, ignoring heartbeat{style_reset}");
                return;
            }
        };

        if let Err(e) = leader_actor.try_send(Ack { term: msg.term }) {
            println!("{color_red}[❤️] Error sending ACK to connection leader: {}{style_reset}", e);
            if let Some(leader_id) = self.leader_id.clone() {
                self.connection_map.remove(&leader_id);
            }
            self.run_election_timer();
        }
    }

}

impl Handler<No> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: No, _ctx: &mut Self::Context) -> Self::Result {
        let vote_term = msg.term;

        if vote_term > self.current_term as u16 {
            println!("[VOTE] I'm out of date, now I become a follower.");
            self.state = State::Follower;
        }
    }
}
impl Handler<NewConnection> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: NewConnection, _ctx: &mut Self::Context) -> Self::Result {

        let actor_addr = HealthConnection::create_actor(msg.stream, msg.id_connection.clone());
        self.connection_map.insert(msg.id_connection, actor_addr.clone());

        if let Some(myself_addr) = &self.myself {
            actor_addr
                .try_send(AddBackend { node: myself_addr.clone() })
                .expect("Error sending AddBackend to accepted connection");
        }
    }
}

impl Handler<Reconnection> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: Reconnection, _ctx: &mut Self::Context) -> Self::Result {
        if self.connection_map.contains_key(&msg.node_id) {
            return;
        }

        println!("{color_green}[CONNECT] Connecting to Node {}{style_reset}", msg.node_id);

        let addr = format!("{}:{}", msg.ip, msg.port);
        let future_stream = TcpStream::connect(addr);

        let self_ip = self.ip.clone();
        let self_port = self.port;
        let node_id = self.node_id.clone();
        let myself_clone = self.myself.clone();
        let ctx_clone = _ctx.address();
        let cur_term = self.current_term;
        let leader_id = self.leader_id.clone();

        actix::spawn(async move {
            match future_stream.await {
                Ok(stream) => {
                    println!("{color_green}[CONNECT] Successfully connected to Node {}{style_reset}", msg.node_id);
                    let actor_addr = HealthConnection::create_actor(stream, msg.node_id.clone());

                    if let Err(e) = ctx_clone
                        .send(AddNode { id: msg.node_id.clone(), node: actor_addr.clone() })
                        .await
                    {
                        eprintln!("Error sending AddNode: {}", e);
                    }

                    if let Some(myself) = myself_clone {
                        if let Err(e) = actor_addr.try_send(AddBackend { node: myself }) {
                            eprintln!("Failed to send AddBackend: {}", e);
                        }
                    }

                    if let Err(e) = actor_addr.try_send(ID {
                        ip: self_ip,
                        port: self_port,
                        id: node_id.clone(),
                        just_arrived: false,
                    }) {
                        eprintln!("Failed to send ID: {}", e);
                    }

                    if let Some(leader_id) = leader_id {
                        if let Err(e) = actor_addr.try_send(NewLeader { id: leader_id, term: cur_term }) {
                            eprintln!("Failed to send NewLeader: {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("Failed to reconnect to Node {}: {}", msg.node_id, e);
                }
            }
        });
    }

}

impl Handler<UpdateID> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: UpdateID, _ctx: &mut Self::Context) -> Self::Result {    
        if self.id_is_connected(&msg.old_id) {
            self.update_id(&msg);
        }
        
        if msg.expects_leader {
            self.try_send_new_leader(msg.new_id.clone());
        }
    }
}

impl Handler<AskIfLeader> for ConsensusModule {
    type Result = bool;

    fn handle(&mut self, _msg: AskIfLeader, _ctx: &mut Self::Context) -> bool {
        self.state == State::Leader
    }
}

impl ConsensusModule {
    fn id_is_connected(&self, id: &str) -> bool {
        self.connection_map.contains_key(id)
    }

    fn update_id(&mut self, msg: &UpdateID) {
        if let Some(connection) = self.connection_map.remove(&msg.old_id) { // si estaba 127.0.0.1:65117, lo borro
            self.connection_map.insert(msg.new_id.clone(), connection); // node2
            return
        }
    }

    fn try_send_new_leader(&self, new_id: String) {
        if self.leader_id.is_none() {
            return;
        }
        let leader_id = match self.leader_id.clone() {
            Some(id) => id,
            None => return,
        };

        if let Some(leader_actor) = self.connection_map.get(&new_id) {
            leader_actor
                .try_send(NewLeader {
                    id: leader_id,
                    term: self.current_term,
                })
                .expect("Error sending NewLeader to new leader");
        }
    }
}