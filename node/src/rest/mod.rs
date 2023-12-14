use crate::network::communication_types::NodeApiRequest;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt;
use tokio::sync::mpsc;
use warp::{Filter, Rejection, Reply};

pub async fn start_rest_api(request_tx: mpsc::Sender<NodeApiRequest>, rest_port: u16) {
    // Define the routes
    let value_route = value_fetch_filter(request_tx.clone());
    let add_key_route = add_key_filter(request_tx);

    let routes = value_route.or(add_key_route);
    warp::serve(routes).run(([0, 0, 0, 0], rest_port)).await;
}

// Define a custom error type (implement necessary traits as required)
#[derive(Debug)]
struct ApiError {
    message: String,
}

impl warp::reject::Reject for ApiError {}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Api Error {:?}", self.message)
    }
}

// Corrected value_fetch_filter function
fn value_fetch_filter(
    request_tx: mpsc::Sender<NodeApiRequest>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("value" / String)
        .and(warp::get())
        .and_then(move |key: String| {
            let request_tx = request_tx.clone();
            async move {
                let (resp_tx, mut resp_rx) = mpsc::channel(1);

                request_tx
                    .send(NodeApiRequest::GetValue {
                        key: key.clone().into_bytes(),
                        resp_chan: resp_tx,
                    })
                    .await
                    .unwrap();

                match resp_rx.recv().await {
                    Some(Ok(value)) => {
                        Ok(warp::reply::json(&json!({ "key": key, "value": value })))
                    }
                    Some(Err(err)) => Err(warp::reject::custom(ApiError {
                        message: err.to_string(),
                    })),
                    None => Err(warp::reject::custom(ApiError {
                        message: "Node response error".to_string(),
                    })),
                }
            }
        })
}

#[derive(Deserialize, Serialize, Debug)]
struct AddKeyExpected {
    key: String,
    value: Vec<u8>,
}

// Corrected add_key_filter function
fn add_key_filter(
    request_tx: mpsc::Sender<NodeApiRequest>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("add-key")
        .and(warp::post())
        .and(warp::body::json::<AddKeyExpected>())
        .and_then(move |incoming: AddKeyExpected| {
            let request_tx = request_tx.clone();
            println!("{:?}", incoming);
            let key = incoming.key;
            let value = incoming.value;

            async move {
                let (resp_tx, mut resp_rx) = mpsc::channel(1);

                request_tx
                    .send(NodeApiRequest::AddNewValue {
                        key: key.clone().into_bytes(),
                        value,
                        resp_chan: resp_tx,
                    })
                    .await
                    .unwrap();

                match resp_rx.recv().await {
                    Some(Ok(())) => Ok(warp::reply::json(
                        &json!({ "message": "Key added successfully" }),
                    )),
                    Some(Err(err)) => Err(warp::reject::custom(ApiError {
                        message: err.to_string(),
                    })),
                    None => Err(warp::reject::custom(ApiError {
                        message: "Node response error".to_string(),
                    })),
                }
            }
        })
}
