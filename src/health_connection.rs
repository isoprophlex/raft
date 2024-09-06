extern crate actix;
use actix::prelude::*;

use crate::backend::ConsensusModule;
use crate::messages::{
    AddBackend, ConnectionDown, Coordinator, Heartbeat, NewLeader, No, RequestAnswer,
    RequestedOurVote, StartElection, Vote, Ack, HB,
};
use actix::fut::wrap_future;
use actix::{Actor, Addr, Context, Handler, StreamHandler};
use std::collections::HashMap;
use tokio::io::{split, AsyncBufReadExt, BufReader};
use tokio::{
    io::{AsyncWriteExt, WriteHalf},
    net::TcpStream,
};
use tokio_stream::wrappers::LinesStream;

/// Actor que maneja las conexiones de salud y comunicación entre nodos.
///
/// Este actor se encarga de recibir y procesar mensajes de otros nodos y enviar respuestas
/// a través de la conexión TCP establecida.
pub struct HealthConnection {
    write: Option<WriteHalf<TcpStream>>,
    id_connection: Option<usize>,
    other_actors: HashMap<usize, Addr<HealthConnection>>,
    backend_actor: Option<Addr<ConsensusModule>>,
    current_term: usize,
}

impl Actor for HealthConnection {
    type Context = Context<Self>;
}

impl StreamHandler<Result<String, std::io::Error>> for HealthConnection {
    /// Maneja los mensajes entrantes desde la conexión TCP.
    ///
    /// Procesa las líneas recibidas, decodifica los comandos y envía los mensajes apropiados
    /// al actor de backend o maneja la desconexión si ocurre un error.
    ///
    /// # Arguments
    /// * `read` - El resultado de la lectura de una línea desde la conexión TCP.
    /// * `ctx` - El contexto del actor.
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
                            backend_addr
                                .clone()
                                .try_send(RequestedOurVote {
                                    candidate_id: candidate_node as usize,
                                    term: term as usize,
                                })
                                .expect("Failed to send message");
                        } else {
                            eprintln!("Backend actor is not available");
                        }
                    }
                }
                Some("VOTE") => {
                    if let Some(Ok(term)) = words.next().map(|w| w.parse::<u16>()) {
                        if let Some(backend_addr) = &self.backend_actor {
                            backend_addr
                                .clone()
                                .try_send(Vote {
                                    id: self.id_connection.unwrap(),
                                    term: term.into(),
                                })
                                .expect("Failed to send message");
                        } else {
                            eprintln!("Backend actor is not available");
                        }
                    }
                }
                Some("HB") => {
                    if let Some(Ok(term)) = words.next().map(|w| w.parse::<u16>()) {
                        let _ = self.backend_actor.clone().unwrap().try_send(HB { term });
                    }
                }
                Some("NO") => {
                    if let Some(Ok(term)) = words.next().map(|w| w.parse::<u16>()) {
                        let _ = self.backend_actor.clone().unwrap().try_send(No { term });
                    }
                }
                Some("NL") => {
                    if let (Some(Ok(leader_node)), Some(Ok(term))) = (
                        words.next().map(|w| w.parse::<u16>()),
                        words.next().map(|w| w.parse::<u16>()),
                    ) {
                        if let Some(backend_addr) = &self.backend_actor {
                            backend_addr
                                .clone()
                                .try_send(NewLeader {
                                    id: leader_node as usize,
                                    term: term as usize,
                                })
                                .expect("Failed to send message");
                        } else {
                            eprintln!("Backend actor is not available");
                        }
                    }
                }
                _ => {
                    println!(
                        "[CONNECTION {:?}] MESSAGE RECEIVED: {:?}",
                        self.id_connection, line
                    )
                }
            }
        } else {
            println!("[ACTOR {}] Connection lost", self.id_connection.unwrap());
            ctx.stop();
        }
    }
}

impl HealthConnection {
    /// Crea un nuevo actor `HealthConnection` a partir de una conexión TCP.
    ///
    /// # Arguments
    /// * `stream` - La conexión TCP que se usará para la comunicación.
    /// * `id_connection` - El identificador de la conexión.
    ///
    /// # Returns
    /// Un `Addr<HealthConnection>` que representa el actor creado.
    pub fn create_actor(stream: TcpStream, id_connection: usize) -> Addr<HealthConnection> {
        HealthConnection::create(|ctx| {
            let (read_half, write_half) = split(stream);
            HealthConnection::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);
            HealthConnection {
                write: Some(write_half),
                id_connection: Some(id_connection),
                other_actors: HashMap::new(),
                backend_actor: None,
                current_term: 0,
            }
        })
    }

    /// Envía una respuesta a través de la conexión TCP.
    ///
    /// # Arguments
    /// * `response` - La respuesta a enviar.
    /// * `ctx` - El contexto del actor.
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
            .map(move |result, this, ctx| match result {
                Ok(write) => {
                    this.write = Some(write);
                }
                Err(_) => {
                    ctx.stop();
                }
            })
            .wait(ctx);
    }
}

impl Handler<StartElection> for HealthConnection {
    type Result = ();

    /// Maneja el mensaje `StartElection` para iniciar una nueva elección.
    ///
    /// Envía una solicitud de voto al actor de backend.
    ///
    /// # Arguments
    /// * `msg` - El mensaje `StartElection` que contiene el identificador del candidato y el término.
    /// * `ctx` - El contexto del actor.
    fn handle(&mut self, msg: StartElection, ctx: &mut Self::Context) -> Self::Result {
        self.make_response(format!("RV {} {}", msg.id, msg.term), ctx);
    }
}

impl Handler<RequestAnswer> for HealthConnection {
    type Result = ();

    /// Maneja el mensaje `RequestAnswer` para responder a una solicitud de voto.
    ///
    /// Actualiza el término actual y envía una respuesta al actor de backend.
    ///
    /// # Arguments
    /// * `msg` - El mensaje `RequestAnswer` que contiene la respuesta y el término.
    /// * `ctx` - El contexto del actor.
    fn handle(&mut self, msg: RequestAnswer, ctx: &mut Self::Context) -> Self::Result {
        self.current_term = msg.term;
        self.make_response(format!("{} {}", msg.msg, msg.term), ctx);
    }
}

impl Handler<AddBackend> for HealthConnection {
    type Result = ();

    /// Maneja el mensaje `AddBackend` para establecer el actor de backend.
    ///
    /// # Arguments
    /// * `msg` - El mensaje `AddBackend` que contiene la dirección del actor de backend.
    /// * `_ctx` - El contexto del actor.
    fn handle(&mut self, msg: AddBackend, _ctx: &mut Self::Context) -> Self::Result {
        self.backend_actor = Some(msg.node);
    }
}

impl Handler<ConnectionDown> for HealthConnection {
    type Result = ();

    /// Maneja el mensaje `ConnectionDown` para eliminar un actor de la lista de actores conectados.
    ///
    /// # Arguments
    /// * `msg` - El mensaje `ConnectionDown` que contiene el identificador del actor que se ha desconectado.
    /// * `_ctx` - El contexto del actor.
    fn handle(&mut self, msg: ConnectionDown, _ctx: &mut Self::Context) -> Self::Result {
        self.other_actors.remove(&msg.id);
    }
}

impl Handler<Ack> for HealthConnection {
    type Result = ();

    /// Maneja el mensaje `Ack` para enviar una confirmación de recepción.
    ///
    /// # Arguments
    /// * `msg` - El mensaje `Ack` que contiene el término.
    /// * `ctx` - El contexto del actor.
    fn handle(&mut self, msg: Ack, ctx: &mut Self::Context) -> Self::Result {
        self.make_response(format!("ACK {}", msg.term), ctx);
    }
}

impl Handler<Heartbeat> for HealthConnection {
    type Result = ();

    /// Maneja el mensaje `Heartbeat` para enviar un latido de corazón.
    ///
    /// # Arguments
    /// * `msg` - El mensaje `Heartbeat` que contiene el término.
    /// * `ctx` - El contexto del actor.
    fn handle(&mut self, msg: Heartbeat, ctx: &mut Self::Context) -> Self::Result {
        self.make_response(format!("HB {}", msg.term), ctx);
    }
}

impl Handler<Coordinator> for HealthConnection {
    type Result = ();

    /// Maneja el mensaje `Coordinator` para anunciar un nuevo líder.
    ///
    /// # Arguments
    /// * `msg` - El mensaje `Coordinator` que contiene el identificador del líder y el término.
    /// * `ctx` - El contexto del actor.
    fn handle(&mut self, msg: Coordinator, ctx: &mut Self::Context) -> Self::Result {
        self.make_response(format!("NL {} {}", msg.id, msg.term), ctx);
    }
}
