use crate::backend::ConsensusModule;
use crate::health_connection::HealthConnection;
use actix::prelude::*;
use tokio::net::TcpStream;

/// Message struct representing the need to add a node connection
#[derive(Message)]
#[rtype(result = "()")]
pub struct AddNode {
    pub id: String,
    pub node: Addr<HealthConnection>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AddBackend {
    pub node: Addr<ConsensusModule>,
}
/// Message representing a Coordinator message
#[derive(Message)]
#[rtype(result = "()")]
pub struct Coordinator {
    pub id: String,
    pub term: usize,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct NewLeader {
    pub id: String,
    pub term: usize,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct Vote {
    pub id: String,
    pub term: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ConnectionDown {
    pub id: String,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct StartElection {
    pub id: String,
    pub term: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Heartbeat {
    pub term: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RequestedOurVote {
    pub term: usize,
    pub candidate_id: String,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct RequestAnswer {
    pub msg: String,
    pub term: usize,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct No {
    pub term: u16,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct HB {
    pub term: u16,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct Ack {
    pub term: u16,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct NewConnection {
    pub id_connection: String,
    pub stream: TcpStream,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct Reconnection {
    pub ip: String,
    pub port: usize,
    pub node_id: String,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct ID {
    pub ip: String,
    pub port: usize,
    pub id: String,
    pub just_arrived: bool,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateID {
    pub ip: String,
    pub port: usize,
    pub old_id: String,
    pub new_id: String,
    pub expects_leader: bool
}

#[derive(Message)]
#[rtype(result = "bool")]
pub struct AskIfLeader;