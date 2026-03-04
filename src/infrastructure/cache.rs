use crate::infrastructure::config::get;
use redis::{AsyncCommands, Client, Cmd, aio::ConnectionManager};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// redis配置类
#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub uri: String,
    pub default_db: u8,
}

impl RedisConfig {
    pub fn config() -> Self {
        let cfg = get();
        let url = cfg.get_string("cache.redis.uri").unwrap_or_default();
        let default_db = cfg.get_int("cache.redis.db0").unwrap_or(0) as u8;
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

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Invalid database index:{0}, must be 0-15")]
    InvalidDB(u8),

    #[error("Connection manager not found for db:{0}")]
    ConnectionManagerNotFound(u8),
}

#[async_trait::async_trait]
pub trait Cache: Send + Sync {
    // 设置缓存
    async fn set<T: Serialize + Send + Sync>(
        &mut self,
        db: u8,
        key: &str,
        value: &T,
        ttl_seconds: u64,
    ) -> Result<(), CacheError>;

    // 从缓存当中获取数据
    async fn get<T: DeserializeOwned>(
        &mut self,
        db: u8,
        key: &str,
    ) -> Result<Option<T>, CacheError>;

    // 删除缓存数据
    async fn del(&mut self, db: u8, key: &str) -> Result<(), CacheError>;
}

#[derive(Clone)]
pub struct RedisCache {
    base_uri: String,
    managers: Arc<tokio::sync::RwLock<HashMap<u8, ConnectionManager>>>,
}

impl RedisCache {
    pub fn new(config: &RedisConfig) -> Result<Self, CacheError> {
        let base_uri = config.uri.to_string();
        let managers = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        Ok(Self { base_uri, managers })
    }

    // 根据传入的db编号对其进行操作
    async fn get_connection(&mut self, db: u8) -> Result<ConnectionManager, CacheError> {
        // 1. 检查db编号
        if db > 15 {
            return Err(CacheError::InvalidDB(db));
        }

        // 2. 尝试加锁读取
        {
            let managers = self.managers.read().await;
            if let Some(manager) = managers.get(&db) {
                return Ok(manager.clone());
            }
        }

        // 3. 写入
        let managers = self.managers.write().await;
        if let Some(manager) = managers.get(&db) {
            return Ok(manager.clone());
        }

        // 4. 连接redis
        let client = Client::open(self.base_uri.to_string())?;
        let mut manager = ConnectionManager::new(client).await?;

        let _: () = Cmd::new()
            .arg("SELECT")
            .arg(db)
            .query_async(&mut manager)
            .await?;

        Ok(manager)
    }

    fn serialize<T: Serialize>(value: &T) -> Result<String, CacheError> {
        serde_json::to_string(value).map_err(|e| CacheError::Serialization(e.to_string()))
    }

    fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Result<T, CacheError> {
        serde_json::from_slice(data).map_err(|e| CacheError::Deserialization(e.to_string()))
    }
}

#[async_trait::async_trait]
impl Cache for RedisCache {
    async fn set<T: Serialize + Send + Sync>(
        &mut self,
        db: u8,
        key: &str,
        value: &T,
        ttl_seconds: u64,
    ) -> Result<(), CacheError> {
        let mut conn = self.get_connection(db).await?;
        let json = Self::serialize(value)?;

        // set/set_ex 也需要指定返回类型 ()
        if ttl_seconds > 0 {
            let _: () = conn.set_ex::<_, _, ()>(key, json, ttl_seconds).await?;
        } else {
            let _: () = conn.set::<_, _, ()>(key, json).await?;
        }
        Ok(())
    }

    async fn get<T: DeserializeOwned>(
        &mut self,
        db: u8,
        key: &str,
    ) -> Result<Option<T>, CacheError> {
        let mut conn = self.get_connection(db).await?;
        let result: Option<Vec<u8>> = conn.get(key).await?;
        match result {
            Some(data) => Ok(Some(Self::deserialize(&data)?)),
            None => Ok(None),
        }
    }

    async fn del(&mut self, db: u8, key: &str) -> Result<(), CacheError> {
        let mut conn = self.get_connection(db).await?;
        let _: () = conn.del::<_, ()>(key).await?;
        Ok(())
    }
}
