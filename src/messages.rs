use actix::prelude::*;
use crate::backend::{ConsensusModule};
use crate::health_connection::HealthConnection;

/// Message struct representing the need to add a node connection
#[derive(Message)]
#[rtype(result = "()")]
pub struct AddNode {
    pub id: usize,
    pub node: Addr<HealthConnection>
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AddBackend {
    pub node: Addr<ConsensusModule>
}
/// Message representing a Coordinator message
#[derive(Message)]
#[rtype(result = "()")]
pub struct Coordinator {
    pub id: usize
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct Vote {
    pub id: usize,
    pub term: usize
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ConnectionDown {
    pub id: usize
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct StartElection {
    pub id: usize,
    pub term: usize
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateTerm {
    pub term: usize
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Heartbeat {
    pub term: usize
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RequestedOurVote {
    pub term: usize,
    pub candidate_id: usize
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct RequestAnswer {
    pub msg: String
}