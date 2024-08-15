extern crate actix;
use actix::prelude::*;

use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;
use actix::{Actor, Addr, Context, Handler, StreamHandler};
use actix::fut::wrap_future;
use tokio::io::{AsyncBufReadExt, split, BufReader};
use tokio_stream::wrappers::LinesStream;
use crate::messages::{AddNode, Coordinator, ConnectionDown, StartElection, CountVotes};
use tokio::{
    io::{AsyncWriteExt, WriteHalf},
    net::TcpStream,
};


pub struct HealthConnection {
    write: Option<WriteHalf<TcpStream>>,
    id_connection: Option<usize>,
    id_node: Option<usize>,
    other_actors: HashMap<usize, Addr<HealthConnection>>,
    votes_earned: usize
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
                        println!("[CONNECTION {:?}] Requested my vote on term {}", self.id_connection, term);
                    }
                }
                _ => {
                    println!("[CONNECTION {:?}] MESSAGE RECEIVED: {:?}", self.id_connection, line)
                }
            }
        }
        else {
            println!("[ACTOR {}] Connection lost", self.id_connection.unwrap());
            let id = self.id_connection.unwrap();
            for (notifying, actor) in &self.other_actors {
                println!("SENDING TO ACTOR: {}", notifying);
                actor.do_send(ConnectionDown { id });
            }
            ctx.stop();
        }
    }
}
impl Handler<AddNode> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: AddNode, ctx: &mut Self::Context) -> Self::Result {
        println!("[ACTOR {}] Added connection with: {}", self.id_connection.unwrap(), msg.id);
        self.other_actors.insert(msg.id, msg.node);
        //DEBUG: println!("Actors connected: {:?}", self.other_actors);
    }
}
impl Handler<Coordinator> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: Coordinator, ctx: &mut Self::Context) -> Self::Result {
        println!("[ACTOR {:?}] Received Coordinator: {}", self.id_connection, msg.id);
        self.make_response(format!("COORDINATOR {}", msg.id), ctx);
    }
}
impl Handler<ConnectionDown> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: ConnectionDown, ctx: &mut Self::Context) -> Self::Result {
        println!("[ACTOR {:?}] Connection down with actor: {}", self.id_connection, msg.id);
        self.other_actors.remove_entry(&msg.id);
        println!("Actor {} deleted", msg.id);
    }
}
impl Handler<StartElection> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: StartElection, ctx: &mut Self::Context) -> Self::Result {
        println!("Requesting vote of: {}" ,self.id_connection.unwrap());
        self.make_response(format!("RV {} {}", msg.id, msg.term), ctx);
    }
}
impl Handler<CountVotes> for HealthConnection {
    type Result = usize;

    fn handle(&mut self, msg: CountVotes, ctx: &mut Self::Context) -> Self::Result {
        self.votes_earned
    }
}
impl HealthConnection {
    pub fn create_actor(stream: TcpStream,  self_id: usize, id_connection: usize) -> Addr<HealthConnection> {
        HealthConnection::create(|ctx| {
            let (read_half, write_half) = split(stream);
            HealthConnection::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);
            HealthConnection {
                write: Some(write_half),
                id_connection: Some(id_connection),
                id_node: Some(self_id),
                other_actors: HashMap::new(),
                votes_earned: 0
            }
        })
    }

    fn make_response(&mut self, response: String, ctx: &mut Context<Self>) {
        let mut write = self.write.take().expect("[ERROR] - NEW MESSAGE RECEIVED");
        let id = self.id_connection.unwrap();
        let other_actors = self.other_actors.clone();
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
                        for (_, actor) in &other_actors {
                            actor.do_send(ConnectionDown { id });
                        }
                        ctx.stop();
                    }
                }
            })
            .wait(ctx);
    }
}