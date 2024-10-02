use crate::backend::State::{Candidate, Follower};
use crate::health_connection::HealthConnection;
use crate::messages::{AddBackend, AddNode, ConnectionDown, Coordinator, Heartbeat, NewLeader, No, RequestAnswer, RequestedOurVote, StartElection, Vote, Ack, HB, NewConnection};
use actix::{Actor, ActorContext, Addr, AsyncContext, Context, Handler, SpawnHandle};
use std::collections::HashMap;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::Instant;

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
    pub connection_map: HashMap<usize, Addr<HealthConnection>>,
    pub node_id: usize,
    pub port: usize,
    pub state: State,
    pub current_term: usize,
    pub election_reset_event: Instant,
    pub votes: Option<u16>,
    pub last_vote: Option<usize>,
    pub leader_id: Option<usize>,
    myself: Option<Addr<ConsensusModule>>,
    pub heartbeat_handle: Option<SpawnHandle>,
    pub heartbeat_check_handle: Option<SpawnHandle>,
}

impl Actor for ConsensusModule {
    type Context = Context<Self>;
}

impl ConsensusModule {
    /// Inicia las conexiones entre nodos en función del `node_id` y `total_nodes`.
    ///
    /// # Arguments
    /// * `node_id` - El identificador del nodo actual.
    /// * `total_nodes` - El número total de nodos en la red.
    /// * `port` - El puerto en el que el nodo escuchará las conexiones.
    ///
    /// # Returns
    /// Un nuevo módulo de consenso con el mapa de conexiones inicializado.
    pub async fn start_connections(node_id: usize, port: usize) -> Self {
        let mut connection_map = HashMap::new();

        if node_id > 1 {
            for previous_id in 1..node_id {
                let addr = format!("127.0.0.1:{}", previous_id + 8000);
                let stream = TcpStream::connect(addr).await.unwrap();
                println!("Node {} connected to Node {}", node_id, previous_id);

                let actor_addr = HealthConnection::create_actor(stream, previous_id, node_id);
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
            leader_id: None,
            heartbeat_handle: None,
            heartbeat_check_handle: None,
        }
    }

    /// Incrementa el término actual y envía mensajes de `StartElection` a los nodos conectados.
    pub fn start_election(&mut self) {
        self.state = Candidate;
        self.current_term += 1;
        println!("NEW CURRENT TERM: {}", self.current_term);

        self.election_reset_event = Instant::now();
        let connection_clone = self.connection_map.clone();
        if connection_clone.len() == 1 {
            self.state = State::Leader;
        }
        for (id, connection) in connection_clone {
            match connection.try_send(StartElection {
                id: self.node_id,
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
    /// Calcula y devuelve el tiempo de espera para la elección.
    ///
    /// # Returns
    /// Un `Duration` con un valor aleatorio para el tiempo de espera.
    pub fn election_timeout(&self) -> Duration {
        Duration::from_millis(1000 + rand::random::<u64>() % 150)
    }
    /// Inicia el temporizador de elecciones y verifica el tiempo transcurrido
    /// para determinar si debe comenzar una nueva elección.
    pub fn run_election_timer(&mut self) {
        let timeout_duration = self.election_timeout();
        let term_started = self.current_term;
        println!(
            "[NODE {}] ELECTION TIMER STARTED {}, TERM: {}",
            self.node_id,
            timeout_duration.as_millis(),
            term_started
        );
        loop {
            if self.state != State::Candidate && self.state != Follower {
                println!("[NODE {}] I'M THE CURRENT LEADER", self.node_id);
                return;
            }
            if term_started != self.current_term {
                println!(
                    "[NODE {}] IN ELECTION TIMER TERM CHANGED FROM {} TO {}",
                    self.node_id, term_started, self.current_term
                );
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
        for connection in self.connection_map.values() {
            connection
                .send(AddBackend {
                    node: ctx.to_owned(),
                })
                .await
                .expect("Error sending backend to connections");
        }
    }
    pub fn check_votes(&mut self, _ctx: &mut Context<Self>, vote_term: u16) {
        println!("I GOT A VOTE ON TERM: {}", vote_term);
        if vote_term > self.current_term as u16 {
            println!("I'm out of date, now I become a follower.");
            self.state = Follower;
            self.votes = Some(0);
            return;
        } else if vote_term as usize == self.current_term {
            self.votes = Some(self.votes.unwrap() + 1);
            println!("Votes earned: {}", self.votes.unwrap());
        }
        if self.votes.unwrap() >= (self.connection_map.len() as u16 / 2) + 1 {
            // Im leader
            self.become_leader(_ctx);
        }
    }
    /// Marca al nodo como líder y comienza a enviar mensajes de latido a los otros nodos.
    ///
    /// # Arguments
    /// * `ctx` - El contexto de Actix necesario para manejar la ejecución asíncrona.
    pub fn become_leader(&mut self, ctx: &mut Context<Self>) {
        self.state = State::Leader;
        self.leader_id = Some(self.node_id);
        println!(
            "Node {} becomes Leader; term={}",
            self.node_id, self.current_term
        );
        self.announce_leader(ctx);

        if let Some(handle) = self.heartbeat_handle.take() {
            ctx.cancel_future(handle);
        }

        let current_term = self.current_term;
        let node_id = self.node_id;
        let mut connection_map = self.connection_map.clone();
        let connection_map2 = self.connection_map.clone();

        let handle = ctx.run_interval(Duration::from_millis(1000), move |actor, ctx| {
            let mut ids_to_delete: Vec<usize> = Vec::new();
            for (id, connection) in &mut connection_map {
                match connection.try_send(Heartbeat { term: current_term }) {
                    Ok(_) => {}
                    Err(e) => {
                        println!(
                            "[ACTOR] Error sending Heartbeat to connection {}: {}",
                            id, e
                        );
                        for notifying_actor in connection_map2.values() {
                            notifying_actor.do_send(ConnectionDown { id: *id });
                        }
                        ids_to_delete.push(*id);
                    }
                }
            }
            if !ids_to_delete.is_empty() {
                for id in ids_to_delete {
                    connection_map.remove_entry(&id);
                }
            }

            if actor.state != State::Leader {
                println!("Node {} is no longer the Leader", node_id);
                ctx.stop();
            }
        });

        self.heartbeat_handle = Some(handle);
    }
    /// Le manda a los actores que se anuncie como lider
    pub fn announce_leader(&mut self, _ctx: &mut Context<Self>) {
        for actor in self.connection_map.values() {
            actor
                .try_send(Coordinator {
                    term: self.current_term,
                    id: self.node_id,
                })
                .unwrap()
        }
    }
    /// Inicia un intervalo para verificar la recepción de latidos.
    ///
    /// Si no se recibe un latido dentro del intervalo especificado, se inicia una nueva elección.
    ///
    /// # Arguments
    /// * `ctx` - El contexto de Actix para la ejecución del actor.
    pub fn start_heartbeat_check(&mut self, ctx: &mut Context<Self>) {
        if let Some(handle) = self.heartbeat_check_handle.take() {
            ctx.cancel_future(handle);
        }

        let timeout_duration = self.election_timeout();
        let node_id = self.node_id;

        let handle = ctx.run_interval(timeout_duration, move |actor, _ctx| {
            let elapsed = Instant::now().duration_since(actor.election_reset_event);
            println!(
                "[NODE {}] Checking heartbeat... Elapsed: {} ms",
                node_id,
                elapsed.as_millis()
            );

            if actor.state == State::Leader {
                println!(
                    "[NODE {}] I'm the leader, stopping heartbeat check",
                    node_id
                );
                _ctx.cancel_future(actor.heartbeat_check_handle.unwrap());
                actor.heartbeat_check_handle = None;
                return;
            }

            if elapsed >= timeout_duration {
                println!(
                    "[NODE {}] No heartbeat received, starting election",
                    node_id
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
        self.connection_map.insert(msg.id, msg.node);
        println!("Node {} added to connections", msg.id);
    }
}
impl Handler<RequestedOurVote> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: RequestedOurVote, _ctx: &mut Context<Self>) -> Self::Result {
        let vote_term = msg.term;
        println!(
            "Received vote request from {} in term {}",
            msg.candidate_id, msg.term
        );
        if self.last_vote.is_none() || self.last_vote == Some(0) {
            println!("First election!");
            self.last_vote = Some(msg.term);
            let lock = self.connection_map.clone();
            let actor = lock.get(&msg.candidate_id).unwrap();
            actor
                .try_send(RequestAnswer {
                    msg: "VOTE".to_string(),
                    term: vote_term,
                })
                .expect("Error sending VOTE HIM to connection");
        } else if vote_term > self.current_term {
            println!("I have to vote!");
            let lock = self.connection_map.clone();
            let actor = lock.get(&msg.candidate_id).unwrap();
            actor
                .try_send(RequestAnswer {
                    msg: "VOTE".to_string(),
                    term: vote_term,
                })
                .expect("Error sending VOTE HIM to connection");
        } else if vote_term == self.current_term {
            println!("I have already voted!");
            let lock = self.connection_map.clone();
            let actor = lock.get(&msg.candidate_id).unwrap();
            actor
                .try_send(RequestAnswer {
                    msg: "NO".to_string(),
                    term: self.current_term,
                })
                .expect("Error sending VOTE HIM to connection");
        }
    }
}
impl Handler<Vote> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: Vote, _ctx: &mut Self::Context) -> Self::Result {
        println!("Received vote from {} in term {}", msg.id, msg.term);
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
        println!("We have a new leader: {}", msg.id);
        self.leader_id = Some(msg.id);
        if self.current_term < msg.term {
            self.current_term = msg.term;
        }
        self.start_heartbeat_check(_ctx);
    }
}
impl Handler<HB> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: HB, _ctx: &mut Self::Context) -> Self::Result {
        if self.current_term < msg.term as usize {
            println!("I was out of date, updating to {}", msg.term);
            self.current_term = msg.term as usize;
        }

        self.election_reset_event = Instant::now();
        println!("Election reset event updated");

        let leader_actor = self.connection_map.get(&self.leader_id.unwrap()).unwrap();
        match leader_actor.try_send(Ack { term: msg.term }) {
            Ok(_) => {}
            Err(e) => {
                println!("[ACTOR] Error sending ACK to connection leader: {}", e);
                self.connection_map.remove_entry(&self.leader_id.unwrap());
                self.run_election_timer();
            }
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
        }
    }
}
impl Handler<NewConnection> for ConsensusModule {
    type Result = ();

    fn handle(&mut self, msg: NewConnection, _ctx: &mut Self::Context) -> Self::Result {
        let actor_addr = HealthConnection::create_actor(msg.stream, msg.id_connection, self.node_id);
        self.connection_map.insert(msg.id_connection, actor_addr);
    }
}
