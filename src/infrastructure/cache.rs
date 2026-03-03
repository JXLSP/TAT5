use crate::infrastructure::config::get;
use anyhow::Ok;
use redis::{Client, RedisError};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub uri: String,
    pub default_db: u8,
}

impl RedisConfig {
    pub fn config() -> Self {
        let cfg = get();
        let url = cfg
            .get_string(&format!("{}.redis.url", "cache".to_string()))
            .unwrap_or_default();
        let default_db = cfg
            .get_int(&format!("{}.redis.db", "cache".to_string()))
            .unwrap_or_default() as u8;
        Self {
            uri: url,
            default_db: default_db,
        }
    }
}

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("Redis error:{0}")]
    Redis(#[from] redis::RedisError),

    #[error("Serialization error:{0}")]
    Serialization(String),

    #[error("Invalid database index:{0}, must be 0-15")]
    InvalidDB(u8),
}

#[async_trait::async_trait]
pub trait Cache: Send + Sync {
    async fn set<T: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
        ttl_seconds: usize,
    ) -> Result<(), RedisError>;

    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, CacheError>;

    async fn del(&self, key: &str) -> Result<(), CacheError>;
}

#[derive(Clone)]
pub struct RedisCache {
    client: Client,
    default_db: u8,
}

impl RedisCache {
    pub fn new(config: &RedisConfig) -> Result<Self, CacheError> {
        let client = Client::open(config.uri.as_str())?;
        let default_db = config.default_db;
        if default_db > 15 {
            return Err(CacheError::InvalidDB(default_db));
        }
        Ok(Self { client, default_db })
    }

    fn get_redis(&self) -> Result<redis::Connection, CacheError> {
        let mut conn = self.client.get_connection();
        if self.default_db != 0 {
            let _: () = redis::cmd("SELECT").arg(self.default_db).query(&mut conn);
        }
        Ok(conn)
    }
}
