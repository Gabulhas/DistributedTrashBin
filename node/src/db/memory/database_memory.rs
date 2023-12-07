use std::collections::HashMap;

pub struct InMemoryDatabase {
    pub map: HashMap<Vec<u8>, Vec<u8>>,
}

impl InMemoryDatabase {
    pub fn new() -> InMemoryDatabase {
        InMemoryDatabase {
            map: HashMap::new(),
        }
    }
}
