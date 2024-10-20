extern crate actix;
use actix::prelude::*;

use crate::backend::ConsensusModule;
use crate::messages::{AddBackend, ConnectionDown, Coordinator, Heartbeat, NewLeader, No, RequestAnswer, RequestedOurVote, StartElection, Vote, Ack, HB, Reconnection, ID, UpdateID};
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
    id_connection: Option<String>,
    other_actors: HashMap<String, Addr<HealthConnection>>,
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
                    if let (Some(candidate_node), Some(term)) = (
                        words.next().and_then(|w| w.parse::<String>().ok()),
                        words.next().and_then(|w| w.parse::<u16>().ok()),
                    ) {
                        if let Some(backend_addr) = &self.backend_actor {
                            let _ = backend_addr
                                .clone()
                                .try_send(RequestedOurVote {
                                    candidate_id: candidate_node,
                                    term: term as usize,
                                });
                        } else {
                            eprintln!("Backend actor is not available");
                        }
                    }
                }
                Some("VOTE") => {
                    if let Some(term) = words.next().and_then(|w| w.parse::<u16>().ok()) {
                        if let Some(backend_addr) = &self.backend_actor {
                            let _ = backend_addr
                                .clone()
                                .try_send(Vote {
                                    id: self.id_connection.clone().unwrap_or_default(),
                                    term: term.into(),
                                });
                        } else {
                            eprintln!("Backend actor is not available");
                        }
                    }
                }
                Some("HB") => {
                    if let Some(term) = words.next().and_then(|w| w.parse::<u16>().ok()) {
                        if let Some(backend_addr) = &self.backend_actor {
                            let _ = backend_addr.clone().try_send(HB { term });
                        }
                    }
                }
                Some("NO") => {
                    if let Some(term) = words.next().and_then(|w| w.parse::<u16>().ok()) {
                        if let Some(backend_addr) = &self.backend_actor {
                            let _ = backend_addr.clone().try_send(No { term });
                        }
                    }
                }
                Some("ID") => {
                    println!("ID received");
                    if let (Some(id), Some(ip), Some(port), Some(expects_leader)) = (
                        words.next().and_then(|w| w.parse::<String>().ok()),
                        words.next().map(|w| w.to_string()),
                        words.next().and_then(|w| w.parse::<usize>().ok()),
                        words.next().and_then(|w| w.parse::<bool>().ok()),
                    ) {
                        if let Some(backend_addr) = &self.backend_actor {
                            let _ = backend_addr.clone().try_send(UpdateID {
                                ip,
                                port,
                                new_id: id.clone(),
                                old_id: self.id_connection.clone().unwrap_or_default(),
                                expects_leader,
                            });

                            if id != self.id_connection.clone().unwrap_or_default() {
                                self.id_connection = Some(id);
                            }
                        }
                    }
                }
                Some("NL") => {
                    if let (Some(leader_node), Some(term)) = (
                        words.next().and_then(|w| w.parse::<String>().ok()),
                        words.next().and_then(|w| w.parse::<u16>().ok()),
                    ) {
                        if let Some(backend_addr) = &self.backend_actor {
                            let _ = backend_addr
                                .clone()
                                .try_send(NewLeader {
                                    id: leader_node,
                                    term: term as usize,
                                });
                        } else {
                            eprintln!("Backend actor is not available");
                        }
                    }
                }
                Some("RC") => {
                    if let (Some(id), Some(ip), Some(port)) = (
                        words.next().and_then(|w| w.parse::<String>().ok()),
                        words.next().map(|w| w.to_string()),
                        words.next().and_then(|w| w.parse::<usize>().ok()),
                    ) {
                        if let Some(backend_addr) = &self.backend_actor {
                            let _ = backend_addr
                                .clone()
                                .try_send(Reconnection {
                                    node_id: id,
                                    ip,
                                    port,
                                });
                        } else {
                            eprintln!("Backend actor is not available");
                        }
                    }
                }
                Some("CD") => {
                    if let Some(node_id) = words.next().and_then(|w| w.parse::<String>().ok()) {
                        if let Some(backend_addr) = &self.backend_actor {
                            let _ = backend_addr
                                .clone()
                                .try_send(ConnectionDown { id: node_id });
                        } else {
                            eprintln!("Backend actor is not available");
                        }
                    }
                }
                _ => {
                    let id_connection = self.id_connection.clone().unwrap_or_else(|| "Unknown".to_string());
                    println!("[{:?}]: {:?}", id_connection, line);
                }
            }
        } else {
            println!("[{:?}] Connection lost", self.id_connection);
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
    pub fn create_actor(stream: TcpStream, id_connection: String) -> Addr<HealthConnection> {
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
        let id = self.id_connection.clone().unwrap();
        wrap_future::<_, Self>(async move {
            match write.write_all(format!("{}\n", response).as_bytes()).await {
                Ok(_) => Ok(write),
                Err(e) => {
                    eprintln!("[ACTOR {}] Failed to send message: {}", id, e);
                    Err(write)
                }
            }
        })
            .map(move |result, this, _ctx| match result {
                Ok(write) => {
                    this.write = Some(write);
                }
                Err(_) => {
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
        self.make_response(format!("CD {}", msg.id), _ctx);
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

impl Handler<Reconnection> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: Reconnection, _ctx: &mut Self::Context) -> Self::Result {
        self.make_response(format!("RC {} {} {} ", msg.node_id, msg.ip, msg.port), _ctx);
    }
}
impl Handler<ID> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: ID, _ctx: &mut Self::Context) -> Self::Result {
        self.make_response(format!("ID {} {} {} {}", msg.id, msg.ip, msg.port, msg.just_arrived), _ctx);
    }
}

impl Handler<NewLeader> for HealthConnection {
    type Result = ();

    fn handle(&mut self, msg: NewLeader, _ctx: &mut Self::Context) -> Self::Result {
        self.make_response(format!("NL {} {}", msg.id, msg.term), _ctx);
    }
}
