use std::thread;

use crate::backend::{self, ConsensusModule};
use crate::messages::{AskIfLeader, NewConnection};
use crate::node_config::{NodesConfig};
use actix::{Addr, AsyncContext, Context, System};
use actix_rt::task;
use futures::executor::block_on;
use tokio::net::{TcpListener, TcpStream};
use utils_lib::*;

const MINIMUM_AMOUNT_FOR_ELECTION: usize = 2;
pub struct RaftModule {
    node_id: String,
    ip: String,
    port: usize,
    pub address: Option<Addr<ConsensusModule>>
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

    pub async fn start(&mut self, nodes_config: NodesConfig, init_history_path: Option<String>) {   

        let node_id = self.node_id.clone();
        let nodes_config_copy = nodes_config.clone();
        let ip = self.ip.clone();
        let port = self.port;

        let handle = actix::spawn(async move {
            println!("[RAFT] start async 1/6");

            let mut backend = ConsensusModule::start_connections(
                ip.clone(),
                port,
                node_id.clone(),
                nodes_config_copy,
                init_history_path
            )
            .await;
        
            let ctx = Context::<ConsensusModule>::new();
            println!("[RAFT] start async 2/6");
            backend.add_myself(ctx.address());
            backend.add_me_to_connections(ctx.address()).await;
            
            println!("[RAFT] start async 3/6");

            let last_node = nodes_config
                .nodes
                .get(MINIMUM_AMOUNT_FOR_ELECTION - 1)
                .unwrap();

            println!("[RAFT] start async 4/6");
            if ip == last_node.ip && port == ((last_node.port.parse::<usize>().unwrap()) + 2000) {
                log!("Running election timer");
                backend.run_election_timer();
            }

            println!("[RAFT] start async 5/6");
            (ctx, backend)
        });

        match handle.await {
            Ok((ctx, backend)) => {
                self.address = Some(ctx.address());
                ctx.run(backend);
            }
            Err(e) => {
                println!("[RAFT] Error starting Raft module: {:?}", e);
            }
        }
        println!("[RAFT] start async 6/6");
    }

    pub fn listen_for_connections(port: usize, ip: String, node_id: String, ctx_task: Addr<ConsensusModule>) {
        println!("[RAFT] Listening for connections on port: {}", port);
        let listener = match std::net::TcpListener::bind(format!("{}:{}", ip, port)) {
            Ok(listener) => listener,
            Err(e) => {
                println!("[RAFT] Error binding listener: {:?}", e);
                return;
            }
        };

        log!("Node {} is listening on {}:{}", node_id, ip, port);
        println!("[RAFT] Node {} is listening on {}:{}", node_id, ip, port);

        RaftModule::accept_connections(listener, ctx_task);
        println!("[RAFT] Listening cnxs, final");
    }

    fn accept_connections(listener: std::net::TcpListener, ctx_task: Addr<ConsensusModule>) {
        println!("[RAFT] Accepting Connections");

        thread::spawn( move || {
            System::new().block_on(async {
                println!("[RAFT] Accepting Connections 2");
                RaftModule::accept_connections_loop(listener, ctx_task).await;
            });
        });
    }

    async fn accept_connections_loop(listener: std::net::TcpListener, ctx_task: Addr<ConsensusModule>) {
        println!("[RAFT] Accepting Connections Loop");
        loop {      
            println!("[RAFT] listening...");
            let result = match listener.accept() {
                Ok((tcp_stream, addr)) => {
                    Some((tcp_stream, addr))
                }
                Err(e) => {
                    println!("[RAFT] couldn't get client: {:?}", e);
                    None
                }
            };

            let (tcp_stream, addr) = match result {
                Some((stream, addr)) => (stream, addr),
                None => {
                    break; // continue?
                }
            };

            log!("Connection accepted from Node: {}", addr);

            let stream = match TcpStream::from_std(tcp_stream) {
                Ok(stream) => stream,
                Err(e) => {
                    println!("[RAFT] Error converting to tokio stream: {:?}", e);
                    continue;
                }
            };
            
            println!("[RAFT] creating msg");;
            let msg = NewConnection {
                id_connection: addr.to_string(),
                stream,
            };

            // Now we send the message using ctx_task.send(msg) inside a thread

            block_on(async {
                _ = ctx_task.send(msg).await;
            });
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
unsafe impl Send for RaftModule {}
