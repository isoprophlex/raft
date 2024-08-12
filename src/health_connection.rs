extern crate actix;
use actix::prelude::*;

use std::collections::HashMap;
use actix::{Actor, Addr, Context, Handler, StreamHandler};
use actix::fut::wrap_future;
use tokio::io::{AsyncBufReadExt, split, BufReader};
use tokio_stream::wrappers::LinesStream;
use crate::messages::{AddNode, Coordinator};
use tokio::{
    io::{AsyncWriteExt, WriteHalf},
    net::TcpStream,
};

/// Raft RPCs
pub enum RPC {
    RequestVotes,
    AppendEntries
}
pub struct HealthConnection {
    write: Option<WriteHalf<TcpStream>>,
    id: Option<usize>,
    other_actors: HashMap<usize, Addr<HealthConnection>>
}
impl Actor for HealthConnection {
    type Context = Context<Self>;
}
impl StreamHandler<Result<String, std::io::Error>> for HealthConnection {
    fn handle(&mut self, read: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(line) = read {
            println!("Received: {}",line);
        } else {
            println!(" Failed to read line");
        }
    }
}
impl Handler<AddNode> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: AddNode, ctx: &mut Self::Context) -> Self::Result {
        println!("[ACTOR {:?}] Added connection with: {}", self.id, msg.id);
        self.other_actors.insert(msg.id, msg.node);
        println!("Actors connected: {:?}", self.other_actors);
    }
}
impl Handler<Coordinator> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: Coordinator, ctx: &mut Self::Context) -> Self::Result {
        println!("[ACTOR {:?}] Received Coordinator: {}", self.id, msg.id);
        self.make_response(format!("COORDINATOR {}", msg.id), ctx);
    }
}
impl HealthConnection {
    pub fn create_actor(stream: TcpStream, id: usize) -> Addr<HealthConnection> {
        HealthConnection::create(|ctx| {
            let (read_half, write_half) = split(stream);
            HealthConnection::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);
            HealthConnection {
                write: Some(write_half),
                id: Some(id),
                other_actors: HashMap::new()
            }
        })
    }

    fn make_response(&mut self, response: String, ctx: &mut Context<Self>) {
        println!("[ACTOR {}] RESPONSE {} ", self.id.unwrap(), response);
        let mut write = self.write.take().expect("[ERROR] - NEW MESSAGE RECEIVED");
        wrap_future::<_, Self>(async move {
            write
                .write_all(format!("{}\n", response).as_bytes())
                .await
                .expect("[ERROR] - MESSAGE SHOULD HAVE SENT");

            write
        })
            .map(|write, this, _| this.write = Some(write)) // Restauramos `write` a `self.write`
            .wait(ctx);
    }

}