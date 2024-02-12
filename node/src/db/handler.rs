use crate::db::{Database, Value};
use tokio::sync::mpsc;

/*
   THIS IS STILL BLOCKING; SYNCRONOUS; SAME THING AS JUST HAVING A MUTEX TO ACCESS THE DATABASE
   LATER THIS SHALL BE USED CONCURRENTLY READ AND WRITE FROM THE DATABASE
*/

pub struct Handler {
    tx: mpsc::Sender<Action>,
}

pub enum ActionType {
    Get(mpsc::Sender<Option<Value>>),
    Insert(Value),
    Delete(mpsc::Sender<Option<Value>>),
    Update(Value, mpsc::Sender<Option<Value>>),
}

pub struct Action {
    key: Vec<u8>,
    action_type: ActionType,
}

impl Handler {
    pub fn start_new(database: Box<dyn Database>) -> Self {
        let (tx, rx) = mpsc::channel::<Action>(32);
        tokio::spawn(Self::start(database, rx));
        Handler { tx }
    }

    pub async fn get(&mut self, key: Vec<u8>) -> Option<Value> {
        let (value_tx, mut value_rx) = mpsc::channel::<Option<Value>>(1);

        self.tx
            .send(Action {
                key,
                action_type: ActionType::Get(value_tx),
            })
            .await
            .unwrap();

        match value_rx.recv().await {
            None => None,
            Some(a) => a,
        }
    }

    pub async fn insert(&mut self, key: Vec<u8>, value: Value) {
        self.tx
            .send(Action {
                key,
                action_type: ActionType::Insert(value),
            })
            .await
            .unwrap()
    }

    pub async fn delete(&mut self, key: Vec<u8>) -> Option<Value> {
        let (value_tx, mut value_rx) = mpsc::channel::<Option<Value>>(1);

        self.tx
            .send(Action {
                key,
                action_type: ActionType::Delete(value_tx),
            })
            .await
            .unwrap();

        match value_rx.recv().await {
            None => None,
            Some(a) => a,
        }
    }

    pub async fn update(&mut self, key: Vec<u8>, value: Value) -> Option<Value> {
        let (value_tx, mut value_rx) = mpsc::channel::<Option<Value>>(1);

        self.tx
            .send(Action {
                key,
                action_type: ActionType::Update(value, value_tx),
            })
            .await
            .unwrap();

        match value_rx.recv().await {
            None => None,
            Some(a) => a,
        }
    }
    pub async fn start(mut database: Box<dyn Database>, mut rx: mpsc::Receiver<Action>) {
        loop {
            tokio::select! {
              request = rx.recv() => match request {
                None => break,
                Some(Action {key, action_type}) => {
                  match action_type {
                    ActionType::Get(tx) => tx.send(database.get(&key)).await.unwrap(),
                    ActionType::Insert(value) => database.insert(key, value),
                    ActionType::Delete(tx) => tx.send(database.delete(&key)).await.unwrap(),
                    ActionType::Update(value, tx) => tx.send(database.update(key, value)).await.unwrap(),

                  };
                }

              },
            }
        }
    }
}
