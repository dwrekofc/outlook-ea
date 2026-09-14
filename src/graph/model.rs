use serde::Serialize;
use thiserror::Error;

use crate::db;

#[derive(Error, Debug)]
pub enum GraphError {
    #[error("Database error: {0}")]
    Db(#[from] db::DbError),
    #[error("Node not found: {0}")]
    NodeNotFound(i64),
    #[error("Edge not found: {0}")]
    EdgeNotFound(i64),
    #[error("Invalid metadata: {0}")]
    Metadata(#[from] serde_json::Error),
}

pub type GraphResult<T> = Result<T, GraphError>;

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: i64,
    pub node_type: String,
    pub name: String,
    pub email: Option<String>,
    pub description: Option<String>,
    pub metadata: String,
    pub is_vip: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub id: i64,
    pub source_id: i64,
    pub target_id: i64,
    pub predicate: String,
    pub context: Option<String>,
    pub weight: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EdgeWithNodes {
    pub edge: Edge,
    pub source: Node,
    pub target: Node,
}

#[derive(Debug, Clone, Serialize)]
pub struct SenderContext {
    pub node_id: i64,
    pub is_vip: bool,
    pub description: Option<String>,
    pub edges: Vec<String>,
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraversalResult {
    pub node: Node,
    pub depth: usize,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleWithDetails {
    pub rule_node: Node,
    pub match_type: Option<String>,
    pub match_value: Option<String>,
    pub action_type: Option<String>,
    pub action_value: Option<String>,
}
