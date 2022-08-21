use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Log
{
    pub timestamp: String,
    pub content: String,
}

pub type Logs = Vec<Log>;

#[derive(Debug, Serialize, Deserialize)]
pub enum ListMode
{
    All,
    One(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListRequest
{
    pub mode: ListMode,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListResponse
{
    pub mode: ListMode,
    pub data: Logs,
    pub receiver: String,
}

pub enum EventType
{
    Response(ListResponse),
    Input(String),
}