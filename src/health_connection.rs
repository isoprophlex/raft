extern crate actix;
use actix::prelude::*;

use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;
use actix::{Actor, Addr, Context, Handler, StreamHandler};
use actix::fut::wrap_future;
use tokio::io::{AsyncBufReadExt, split, BufReader};
use tokio_stream::wrappers::LinesStream;
use crate::messages::{AddNode, Coordinator, ConnectionDown};
use tokio::{
    io::{AsyncWriteExt, WriteHalf},
    net::TcpStream,
};

/// Raft RPCs
pub enum RPC {
    RequestVotes,
    AppendEntries
}
pub enum State {
    Follower,
    Leader,
    Candidate
}
pub struct HealthConnection {
    write: Option<WriteHalf<TcpStream>>,
    id_connection: Option<usize>,
    id_node: Option<usize>,
    other_actors: HashMap<usize, Addr<HealthConnection>>,
    current_state: Option<State>
}
impl Actor for HealthConnection {
    type Context = Context<Self>;
}
impl StreamHandler<Result<String, std::io::Error>> for HealthConnection {
    fn handle(&mut self, read: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        match read {
            Ok(line) => {
                println!("[CONNECTION WITH {}] Received: {}", self.id_connection.unwrap(), line);
                self.make_response("ACK".to_string(), ctx);
            }
            Err(_) => {
                println!("[ACTOR {}] Connection lost", self.id_connection.unwrap());
                // Notificar a otros actores que la conexión se ha caído
                let id = self.id_connection.unwrap();
                for (notifying, actor) in &self.other_actors {
                    println!("SENDING TO ACTOR: {}", notifying);
                    actor.do_send(ConnectionDown { id });
                }
                // Detener el actor actual
                ctx.stop();
            }
        }
    }
}
impl Handler<AddNode> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: AddNode, ctx: &mut Self::Context) -> Self::Result {
        println!("[ACTOR {:?}] Added connection with: {}", self.id_connection, msg.id);
        self.other_actors.insert(msg.id, msg.node);
        println!("Actors connected: {:?}", self.other_actors);
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
impl HealthConnection {
    pub fn create_actor(stream: TcpStream,  self_id: usize, id_connection: usize, initial_state : Option<State>) -> Addr<HealthConnection> {
        HealthConnection::create(|ctx| {
            let (read_half, write_half) = split(stream);
            HealthConnection::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);
            HealthConnection {
                write: Some(write_half),
                id_connection: Some(id_connection),
                id_node: Some(self_id),
                other_actors: HashMap::new(),
                current_state: initial_state,
            }
        })
    }

    fn make_response(&mut self, response: String, ctx: &mut Context<Self>) {
        println!("[ACTOR {}] RESPONSE {} ", self.id_connection.unwrap(), response);

        // Extraemos el write fuera del futuro para evitar problemas de lifetime
        let mut write = self.write.take().expect("[ERROR] - NEW MESSAGE RECEIVED");
        let id = self.id_connection.unwrap();
        let other_actors = self.other_actors.clone(); // Clonamos el mapa para usarlo dentro del futuro

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
                        // Restauramos write en self.write si no hubo errores
                        this.write = Some(write);
                    }
                    Err(_) => {
                        // Notificamos a otros actores de la caída de la conexión
                        for (other_id, actor) in &other_actors {
                            actor.do_send(ConnectionDown { id });
                        }
                        ctx.stop();
                    }
                }
            })
            .wait(ctx);
    }



}