use crate::db::Value;
use std::collections::HashMap;

pub struct InMemoryDatabase {
    pub map: HashMap<Vec<u8>, Value>,
}

impl InMemoryDatabase {
    pub fn new() -> InMemoryDatabase {
        InMemoryDatabase {
            map: HashMap::new(),
        }
    }
}
