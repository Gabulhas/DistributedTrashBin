use std::sync::Arc;
use tokio::sync::mpsc;

use std::future::Future;
use std::pin::Pin;

pub trait AsyncHandler<K, V, S: Storage<K, V> + Send + Sync> {
    fn new(storage: Arc<S>) -> Self
    where
        Self: Sized;

    fn get(&self, key: K) -> Pin<Box<dyn Future<Output = Option<V>> + Send>>;
    fn insert(&self, key: K, value: V) -> Pin<Box<dyn Future<Output = ()> + Send>>;
    fn update(&self, key: K, value: V) -> Pin<Box<dyn Future<Output = Option<V>> + Send>>;
    fn delete(&self, key: K) -> Pin<Box<dyn Future<Output = Option<V>> + Send>>;
}

pub trait Storage<K, V> {
    fn insert(&mut self, key: K, value: V) -> Option<V>;
    fn get(&self, key: &K) -> Option<&V>;
    fn remove(&mut self, key: &K) -> Option<V>;
    // Add update method if needed
}

// Handler action enum
pub enum HandlerAction<K, V> {
    Get(K, mpsc::Sender<Option<V>>),
    Insert(K, V),
    Update(K, V, mpsc::Sender<Option<V>>),
    Delete(K, mpsc::Sender<Option<V>>),
}

pub struct GenericAsyncHandler<K, V, S>
where
    S: Storage<K, V> + Send + Sync,
{
    storage: Arc<S>,
    tx: mpsc::Sender<HandlerAction<K, V>>,
}

impl<K, V, S> AsyncHandler<K, V, S> for GenericAsyncHandler<K, V, S>
where
    K: Send + Clone + 'static,
    V: Send + Clone + 'static,
    S: Storage<K, V> + Send + Sync + 'static,
{
    fn new(storage: Arc<S>) -> Self {
        let (tx, mut rx) = mpsc::channel::<HandlerAction<K, V>>(32);

        let storage_clone = Arc::clone(&storage);

        tokio::spawn(async move {
            while let Some(action) = rx.recv().await {
                match action {
                    HandlerAction::Get(key, resp_tx) => {
                        let result = storage_clone.get(&key).cloned();
                        let _ = resp_tx.send(result).await;
                    }
                    HandlerAction::Insert(key, value) => {
                        storage_clone.insert(key, value);
                    }
                    HandlerAction::Update(key, value, resp_tx) => {
                        // Assuming you add an update method to the Storage trait
                        let result = storage_clone.insert(key, value);
                        let _ = resp_tx.send(result).await;
                    }
                    HandlerAction::Delete(key, resp_tx) => {
                        let result = storage_clone.remove(&key);
                        let _ = resp_tx.send(result).await;
                    }
                }
            }
        });

        GenericAsyncHandler { storage, tx }
    }
    fn get(&self, key: K) -> Pin<Box<dyn Future<Output = Option<V>> + Send>> {
        let (resp_tx, mut resp_rx) = mpsc::channel::<Option<V>>(1);
        let tx = self.tx.clone();
        Box::pin(async move {
            tx.send(HandlerAction::Get(key, resp_tx)).await.unwrap();
            resp_rx.recv().await.unwrap()
        })
    }

    fn insert(&self, key: K, value: V) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let tx = self.tx.clone();
        Box::pin(async move {
            tx.send(HandlerAction::Insert(key, value)).await.unwrap();
        })
    }

    fn update(&self, key: K, value: V) -> Pin<Box<dyn Future<Output = Option<V>> + Send>> {
        let (resp_tx, mut resp_rx) = mpsc::channel::<Option<V>>(1);
        let tx = self.tx.clone();
        Box::pin(async move {
            tx.send(HandlerAction::Update(key, value, resp_tx))
                .await
                .unwrap();
            resp_rx.recv().await.unwrap()
        })
    }

    fn delete(&self, key: K) -> Pin<Box<dyn Future<Output = Option<V>> + Send>> {
        let (resp_tx, mut resp_rx) = mpsc::channel::<Option<V>>(1);
        let tx = self.tx.clone();
        Box::pin(async move {
            tx.send(HandlerAction::Delete(key, resp_tx)).await.unwrap();
            resp_rx.recv().await.unwrap()
        })
    }
}
