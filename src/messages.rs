use crate::backend::ConsensusModule;
use crate::health_connection::HealthConnection;
use actix::prelude::*;
use tokio::net::TcpStream;

#[derive(Message)]
#[rtype(result = "()")]
/// Message struct representing the need to add a node connection
pub struct AddNode {
    pub id: String,
    pub node: Addr<HealthConnection>,
}

#[derive(Message)]
#[rtype(result = "()")]
/// Message struct representing the need to add a connection to the backend
pub struct AddBackend {
    pub node: Addr<ConsensusModule>,
}
#[derive(Message)]
#[rtype(result = "()")]
/// Message representing a Coordinator message. It is used to inform the other nodes that a new leader has been elected.
pub struct Coordinator {
    pub id: String,
    pub term: usize,
}
#[derive(Message)]
#[rtype(result = "()")]
/// Message used to send information of NewLeader.
pub struct NewLeader {
    pub id: String,
    pub term: usize,
}
#[derive(Message)]
#[rtype(result = "()")]
/// Request to vote for a candidate (the node that sent this message)
pub struct Vote {
    pub id: String,
    pub term: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
/// Message struct representing the need to remove a node connection
pub struct ConnectionDown {
    pub id: String,
}
#[derive(Message)]
#[rtype(result = "()")]
/// Request to start an election
pub struct StartElection {
    pub id: String,
    pub term: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
/// Heartbeat sent by the leader to the other nodes to inform that it is still alive
pub struct Heartbeat {
    pub term: usize,
    pub id: String,
}

#[derive(Message)]
#[rtype(result = "()")]
/// Message struct representing that a vote request has been received
pub struct RequestedOurVote {
    pub term: usize,
    pub candidate_id: String,
}
#[derive(Message)]
#[rtype(result = "()")]
/// Representation of the Requested Vote Answer
pub struct RequestAnswer {
    pub msg: String,
    pub term: usize,
}
#[derive(Message)]
#[rtype(result = "()")]
/// Denial of vote request, probably because the node starting an election is out of term
pub struct No {
    pub term: u16,
}

#[derive(Message)]
#[rtype(result = "()")]
/// Internal Message between health_connection and backend, representing a heartbeat
pub struct HB {
    pub term: u16,
    pub id: String,
}
#[derive(Message)]
#[rtype(result = "()")]
/// Message representing an ACK of a heartbeat
pub struct Ack {
    pub term: u16,
}
#[derive(Message)]
#[rtype(result = "()")]
/// Message representing a connection to a new node
pub struct NewConnection {
    pub id_connection: String,
    pub stream: TcpStream,
}
#[derive(Message)]
#[rtype(result = "()")]
/// Message representing a reconnection to a node
pub struct Reconnection {
    pub ip: String,
    pub port: usize,
    pub node_id: String,
}
#[derive(Message)]
#[rtype(result = "()")]
/// Message representing an ID. This is used to inform in the handshake when a node has arrived.
pub struct ID {
    pub ip: String,
    pub port: usize,
    pub id: String,
    pub just_arrived: bool,
}
#[derive(Message)]
#[rtype(result = "()")]
/// Message representing an update of an ID.
pub struct UpdateID {
    pub ip: String,
    pub port: usize,
    pub old_id: String,
    pub new_id: String,
    pub expects_leader: bool,
}

#[derive(Message)]
#[rtype(result = "()")]
/// Message representing the change of role of a node
pub struct RoleChanged {
    pub id: String,
}
