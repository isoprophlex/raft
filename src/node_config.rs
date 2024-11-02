use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct NodesConfig {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Node {
    pub ip: String,
    pub port: String,
    pub name: String,
}
