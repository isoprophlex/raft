use actix::prelude::*;
use crate::health_connection::HealthConnection;

/// Message struct representing the need to add a node connection
#[derive(Message)]
#[rtype(result = "()")]
pub struct AddNode {
    pub id: usize,
    pub node: Addr<HealthConnection>
}
/// Message representing a Coordinator message
#[derive(Message)]
#[rtype(result = "()")]
pub struct Coordinator {
    pub id: usize
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
#[rtype(result = "usize")]
pub struct CountVotes {
    pub term: usize
}