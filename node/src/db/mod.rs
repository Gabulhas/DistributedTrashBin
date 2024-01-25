use crate::utils::async_store_handler::Storage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub mod memory;
// pub mod disk;

// Re-export InMemoryDatabase and DiskDatabase
// pub use disk::DiskDatabase;
pub use memory::InMemoryDatabase;

// If you also have a Database trait

// Implement the trait for the exported structs

pub mod handler;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Value {
    Direct(Vec<u8>), // Direct value
    Pointer(String), // Pointer to another node (e.g., node ID or address)
}

impl Database for InMemoryDatabase {
    fn get(&self, key: &[u8]) -> Option<Value> {
        self.map.get(key).cloned()
    }

    fn insert(&mut self, key: Vec<u8>, value: Value) {
        self.map.insert(key, value);
    }

    fn delete(&mut self, key: &[u8]) -> Option<Value> {
        self.map.remove(key)
    }

    fn update(&mut self, key: Vec<u8>, value: Value) -> Option<Value> {
        self.map.insert(key, value)
    }
}

//impl Database for DiskDatabase {
//    // trait implementation
//}

pub enum DatabaseType {
    InMemory(InMemoryDatabase),
    //Disk(DiskDatabase),
}

pub trait Database: Send + Sync {
    fn get(&self, key: &[u8]) -> Option<Value>;
    fn insert(&mut self, key: Vec<u8>, value: Value);
    fn delete(&mut self, key: &[u8]) -> Option<Value>;
    fn update(&mut self, key: Vec<u8>, value: Value) -> Option<Value>;
}

impl Database for DatabaseType {
    fn get(&self, key: &[u8]) -> Option<Value> {
        match self {
            DatabaseType::InMemory(db) => db.get(key),
            //DatabaseType::Disk(db) => db.get(key),
        }
    }
    fn insert(&mut self, key: Vec<u8>, value: Value) {
        match self {
            DatabaseType::InMemory(db) => db.insert(key, value),
            // DatabaseType::Disk(db) => db.insert(key, value),
        }
    }
    fn delete(&mut self, key: &[u8]) -> Option<Value> {
        match self {
            DatabaseType::InMemory(db) => db.delete(key),
            // DatabaseType::Disk(db) => db.insert(key, value),
        }
    }
    fn update(&mut self, key: Vec<u8>, value: Value) -> Option<Value> {
        match self {
            DatabaseType::InMemory(db) => db.update(key, value),
            // DatabaseType::Disk(db) => db.insert(key, value),
        }
    }

    // Implement insert, delete, and update in a similar fashion
}

impl DatabaseType {
    pub fn create(in_memory: bool) -> Box<dyn Database> {
        if in_memory {
            Box::new(DatabaseType::InMemory(InMemoryDatabase::new()))
        } else {
            panic!("Not implemented yet")
            // Box::new(DatabaseType::Disk(DiskDatabase::new("path/to/db/file")))
        }
    }
}

impl<K, V> Storage<K, V> for Arc<dyn Database>
where
    K: AsRef<[u8]> + Send + Sync,
    V: From<Value> + Into<Value> + Send + Sync,
{
    fn get(&self, key: &K) -> Option<V> {
        self.as_ref().get(key.as_ref()).map(V::from)
    }

    fn insert(&self, key: K, value: V) -> Option<V> {
        self.as_ref().insert(key.as_ref().to_vec(), value.into());
        self.get(&key)
    }

    fn remove(&self, key: &K) -> Option<V> {
        self.as_ref().delete(key.as_ref()).map(V::from)
    }
}
