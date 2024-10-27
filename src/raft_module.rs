use crate::backend::ConsensusModule;
use crate::messages::{AskIfLeader, NewConnection};
use crate::node_config::{NodesConfig};
use actix::{Addr, AsyncContext, Context};
use actix_rt::spawn;
use tokio::net::TcpListener;

const MINIMUM_AMOUNT_FOR_ELECTION: usize = 2;
pub struct RaftModule {
    node_id: String,
    ip: String,
    port: usize,
    address: Option<Addr<ConsensusModule>>
}

impl RaftModule {
    pub fn new(node_id: String, ip: String, port: usize) -> Self {
        RaftModule {
            node_id,
            ip,
            port,
            address: None,
        }
    }

    pub async fn start(&mut self, nodes_config: NodesConfig, timestamp_dir: Option<&str>) {
        let node_id = self.node_id.clone();

        let nodes_config_copy = nodes_config.clone();

        let ctx = Context::<ConsensusModule>::new();
        let mut backend = ConsensusModule::start_connections(
            self.ip.clone(),
            self.port,
            self.node_id.clone(),
            nodes_config_copy,
            timestamp_dir
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

        self.address = Some(ctx.address());

        let last_node = nodes_config
            .nodes
            .get(MINIMUM_AMOUNT_FOR_ELECTION - 1)
            .unwrap();

        if self.ip == last_node.ip && self.port == last_node.port.parse().unwrap() {
            println!("Running election timer");
            backend.run_election_timer();
        }

        ctx.run(backend);
        join.await.expect("Error in join.await");
    }

    pub async fn listen_for_connections(
        node_id: String,
        ip: String,
        port: usize,
        ctx_task: Addr<ConsensusModule>,
    ) {
        let port = port + 3000;
        let listener = TcpListener::bind(format!("{}:{}", ip, port))
            .await
            .expect("Failed to bind listener");

        println!("Node {} is listening on {}:{}", ip, node_id, port);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    println!("Connection accepted from Node: {}", addr);
                    ctx_task
                        .send(NewConnection {
                            id_connection: addr.to_string(),
                            stream,
                        })
                        .await
                        .expect("Error sending new message");
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e);
                }
            }
        }
    }

    pub async fn is_leader(&self) -> bool {
        if let Some(backend) = &self.address {
            return match backend.send(AskIfLeader {}).await {
                Ok(is_leader) => {
                    return is_leader;
                }
                Err(_) => false,
            };
        }
        false
    }
}