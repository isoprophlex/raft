use crate::backend::ConsensusModule;
use crate::messages::{NewConnection};
use crate::node_config::NodesConfig;
use actix::{Addr, AsyncContext, Context};
use std::sync::mpsc::{Sender, Receiver};
use tokio::net::TcpListener;
use utils_lib::*;

const MINIMUM_AMOUNT_FOR_ELECTION: usize = 2;
pub const PORT_OFFSET: usize = 2000;

/// RaftModule is the main struct that will be used to start the Raft consensus algorithm.
pub struct RaftModule {
    node_id: String,
    ip: String,
    port: usize,
    address: Option<Addr<ConsensusModule>>
}

/// Implementation of RaftModule
impl RaftModule {
    /// Creates a new RaftModule.
    pub fn new(node_id: String, ip: String, port: usize) -> Self {
        RaftModule {
            node_id,
            ip,
            port,
            address: None
        }
    }

    /// Starts the RaftModule. This function will start the connections with the other nodes and listen for incoming connections.
    /// It will also start the election timer if the node is the second to go live.
    /// Elections need at least 2 nodes to start.
    /// wait_for_acks will be false only in local testing instances
    pub async fn start(
        &mut self,
        nodes_config: NodesConfig,
        timestamp_dir: Option<&str>,
        sender: Sender<bool>,
        receiver: Receiver<bool>,
        wait_for_acks: bool
    ) {
        let node_id = self.node_id.clone();

        let nodes_config_copy = nodes_config.clone();

        let ctx = Context::<ConsensusModule>::new();
        let mut backend = ConsensusModule::start_connections(
            self.ip.clone(),
            self.port,
            self.node_id.clone(),
            nodes_config_copy,
            timestamp_dir,
            sender,
            receiver,
            wait_for_acks
        )
        .await;

        let join = tokio::spawn(RaftModule::listen_for_connections(
            node_id,
            self.ip.clone(),
            self.port,
            ctx.address(),
        ));
        backend.add_myself(ctx.address());
        backend.add_me_to_connections(ctx.address()).await;
        backend.handshake_nodes();

        self.address = Some(ctx.address());

        let runner_node = nodes_config
            .nodes
            .get(MINIMUM_AMOUNT_FOR_ELECTION - 1)
            .unwrap();

        // If it's my turn to start the election timer, I will start it.
        if self.ip == runner_node.ip && self.port == runner_node.port.parse().unwrap() {
            log!("Running election timer");
            backend.run_election_timer();
        }

        ctx.run(backend);
        join.await.expect("Error in join.await");
    }

    /// Listens for incoming connections.
    /// This function will listen for incoming connections and send them to the ConsensusModule (backend) with the Message NewConnection.
    /// The backend will handle the new connection.
    pub async fn listen_for_connections(
        node_id: String,
        ip: String,
        port: usize,
        ctx_task: Addr<ConsensusModule>,
    ) {
        let port = port + PORT_OFFSET;
        let listener = TcpListener::bind(format!("{}:{}", ip, port))
            .await
            .expect("Failed to bind listener");

        log!("Node {} is listening on {}:{}", node_id, ip, port);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    log!("Connection accepted from Node: {}", addr);
                    ctx_task
                        .send(NewConnection {
                            id_connection: addr.to_string(),
                            stream,
                        })
                        .await
                        .expect("Error sending new message");
                }
                Err(e) => {
                    log!("Error accepting connection: {}", e);
                }
            }
        }
    }
}
unsafe impl Send for RaftModule {}
