extern crate actix;
use actix::prelude::*;

use std::collections::HashMap;
use std::time::Duration;
use actix::{Actor, Addr, Context, Handler, StreamHandler};
use actix::fut::wrap_future;
use tokio::io::{AsyncBufReadExt, split, BufReader};
use tokio_stream::wrappers::LinesStream;
use crate::messages::{AddNode, Coordinator, ConnectionDown, StartElection, AddBackend, Vote, Heartbeat, UpdateTerm, RequestedOurVote, RequestAnswer, No};
use tokio::{
    io::{AsyncWriteExt, WriteHalf},
    net::TcpStream,
};
use crate::backend::ConsensusModule;

pub struct HealthConnection {
    write: Option<WriteHalf<TcpStream>>,
    id_connection: Option<usize>,
    id_node: Option<usize>,
    other_actors: HashMap<usize, Addr<HealthConnection>>,
    votes_earned: usize,
    backend_actor: Option<Addr<ConsensusModule>>, // Cambiado a Option<Addr<ConsensusModule>>
    current_term: usize,
}

impl Actor for HealthConnection {
    type Context = Context<Self>;
}

impl StreamHandler<Result<String, std::io::Error>> for HealthConnection {
    fn handle(&mut self, read: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(line) = read {
            let mut words = line.split_whitespace();
            match words.next() {
                Some("RV") => {
                    if let (Some(Ok(candidate_node)), Some(Ok(term))) = (
                        words.next().map(|w| w.parse::<u16>()),
                        words.next().map(|w| w.parse::<u16>()),
                    ) {
                        if let Some(backend_addr) = &self.backend_actor {
                            backend_addr.clone().try_send(RequestedOurVote {
                                candidate_id: candidate_node as usize,
                                term: term as usize,
                            }).expect("Failed to send message");
                        } else {
                            eprintln!("Backend actor is not available");
                        }
                    }
                }
                Some("VOTE") => {
                    if let Some(Ok(term)) = words.next().map(|w| w.parse::<u16>()) {
                        println!("[CONNECTION {:?}] VOTED ME ON TERM {}", self.id_connection, term);
                        if let Some(backend_addr) = &self.backend_actor {
                            backend_addr.clone().try_send(Vote {
                                id: self.id_connection.unwrap(),
                                term: term.into(),
                            }).expect("Failed to send message");
                        } else {
                            eprintln!("Backend actor is not available");
                        }
                    }
                }
                Some("HB") => {
                    if let Some(Ok(term)) = words.next().map(|w| w.parse::<u16>()) {
                        println!("[CONNECTION {:?}] HEARTBEAT ARRIVED ON TERM {}", self.id_connection, term);
                        self.make_response(format!("ACK {}", self.current_term), ctx);
                    }
                }
                Some("NO") => {
                    if let Some(Ok(term)) = words.next().map(|w| w.parse::<u16>()) {
                        println!("[CONNECTION {:?}] DIDNT VOTE US {}", self.id_connection, term);
                    }
                }
                _ => {
                    println!("[CONNECTION {:?}] MESSAGE RECEIVED: {:?}", self.id_connection, line)
                }
            }
        } else {
            println!("[ACTOR {}] Connection lost", self.id_connection.unwrap());
            // self.handle_disconnect(ctx);
        }
    }
}

impl HealthConnection {
    pub fn create_actor(stream: TcpStream, self_id: usize, id_connection: usize) -> Addr<HealthConnection> {
        HealthConnection::create(|ctx| {
            let (read_half, write_half) = split(stream);
            HealthConnection::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);
            HealthConnection {
                write: Some(write_half),
                id_connection: Some(id_connection),
                id_node: Some(self_id),
                other_actors: HashMap::new(),
                votes_earned: 0,
                backend_actor: None, // Inicializar como None
                current_term: 0,
            }
        })
    }

    fn make_response(&mut self, response: String, ctx: &mut Context<Self>) {
        let mut write = self.write.take().expect("[ERROR] - NEW MESSAGE RECEIVED");
        let id = self.id_connection.unwrap();
        wrap_future::<_, Self>(async move {
            match write.write_all(format!("{}\n", response).as_bytes()).await {
                Ok(_) => Ok(write),
                Err(e) => {
                    println!("[ACTOR {}] Failed to send message: {}", id, e);
                    Err(write)
                }
            }
        })
            .map(move |result, this, ctx| {
                match result {
                    Ok(write) => {
                        this.write = Some(write);
                    }
                    Err(_) => {
                        println!("MURIO");
                    }
                }
            })
            .wait(ctx);
    }
}

impl Handler<StartElection> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: StartElection, ctx: &mut Self::Context) -> Self::Result {
        println!("Requesting vote of: {} on term {}", self.id_connection.unwrap(), msg.term);
        self.make_response(format!("RV {} {}", msg.id, msg.term), ctx);
    }
}

impl Handler<RequestAnswer> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: RequestAnswer, ctx: &mut Self::Context) -> Self::Result {
        self.current_term = msg.term;
        println!("[CONNECTION {}] Term updated to {}", self.id_connection.unwrap(), self.current_term);
        self.make_response(format!("{} {}", msg.msg, msg.term), ctx);
    }
}

impl Handler<AddBackend> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: AddBackend, ctx: &mut Self::Context) -> Self::Result {
        self.backend_actor = Some(msg.node);
    }
}

impl Handler<ConnectionDown> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: ConnectionDown, ctx: &mut Self::Context) -> Self::Result {
        println!("[ACTOR {:?}] Connection down with actor: {}", self.id_connection, msg.id);
        self.other_actors.remove(&msg.id);
    }
}

impl Handler<Heartbeat> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: Heartbeat, ctx: &mut Self::Context) -> Self::Result {
        self.make_response(format!("HB {}", msg.term), ctx);
    }
}
