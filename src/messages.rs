use actix::prelude::*;
use crate::health_connection::HealthConnection;

/// Message struct representing a payment preparation request received from the terminal.
#[derive(Message)]
#[rtype(result = "()")]
pub struct AddNode {
    pub id: usize,
    pub node: Addr<HealthConnection>
}