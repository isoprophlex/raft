use std::env;
use actix::{Actor, Addr, Context, StreamHandler};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, split, WriteHalf, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::wrappers::LinesStream;
use std::sync::{Arc, Mutex};
use tokio::net::unix::SocketAddr;
use tokio::task;
use crate::health_connection::HealthConnection;
use crate::messages::AddNode;

mod health_connection;
mod messages;

#[actix_rt::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <node_id> <total_nodes>", args[0]);
        std::process::exit(1);
    }

    let id: usize = args[1].parse().expect("Invalid ID, must be a number");
    let total_nodes: usize = args[2].parse().expect("Invalid total_nodes, must be a number");
    let port = id + 8000;

    let connection_map: Arc<Mutex<HashMap<usize, Addr<HealthConnection>>>> = Arc::new(Mutex::new(HashMap::new()));
    // First node only accepts
    if id == 1 {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();
        println!("Node {} is listening on 127.0.0.1:{}", id, port);
        let connection_map_clone = Arc::clone(&connection_map);
        let _ = task::spawn_local(async move {
            let mut next_id = id + 1;
            // Accept every connection
            while next_id <= total_nodes {
                if let Ok((stream, _)) = listener.accept().await {
                    let addr = HealthConnection::create_actor(stream, next_id);
                    connection_map_clone.lock().unwrap().insert(next_id, addr);
                    println!("Connection accepted with ID: {}", next_id);
                    println!("Connections: {:?}", connection_map_clone.lock());
                    next_id += 1;
                }
                if !connection_map_clone.lock().unwrap().is_empty() {
                    let hashmap = connection_map_clone.lock().unwrap();
                    for (id, connection) in hashmap.iter() {
                        for (id_actor, to_send) in hashmap.iter() {
                            if id_actor != id {
                                connection.send(AddNode {id: id_actor.clone(), node: to_send.clone()}).await.expect("Error sending AddNode");
                            }
                        }
                    }
                }
            }
        }).await;
    } else if id > 1 && id < total_nodes {
        for previous_id in 1..id {
            let addr = format!("127.0.0.1:{}", previous_id + 8000);
            let stream = TcpStream::connect(addr).await.unwrap();
            println!("Node {} connected to Node {}", id, previous_id);

            // Crear actor para la conexión a cada nodo anterior
            let actor_addr = HealthConnection::create_actor(stream, id);
            connection_map.lock().unwrap().insert(previous_id, actor_addr);
            println!("Connections: {:?}", connection_map.lock());
        }

        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();
        println!("Node {} is listening on 127.0.0.1:{}", id, port);

        let connection_map_clone = Arc::clone(&connection_map);
        let _ = task::spawn_local(async move {
            let mut next_id = id + 1;
            while next_id <= total_nodes {
                if let Ok((stream, _)) = listener.accept().await {
                    let addr = HealthConnection::create_actor(stream, next_id);

                    connection_map_clone.lock().unwrap().insert(next_id, addr);
                    println!("Connection accepted with ID: {}", next_id);
                    next_id += 1;
                }
            }
        }).await;
    } else if id == total_nodes {
        // Nodo N: Solo se conecta a todos los nodos anteriores
        for previous_id in 1..id {
            let addr = format!("127.0.0.1:{}", previous_id + 8000);
            let stream = TcpStream::connect(addr).await.unwrap();
            println!("Node {} connected to Node {}", id, previous_id);

            // Crear actor para cada conexión
            let actor_addr = HealthConnection::create_actor(stream, id);
            connection_map.lock().unwrap().insert(previous_id, actor_addr);
        }
        // No aceptar conexiones
    }
}