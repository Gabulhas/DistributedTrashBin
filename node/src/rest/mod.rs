use crate::network::node::{NodeApiRequest, NodeApiResponse};
use serde_json::json;
use tokio::sync::mpsc;
use warp::{http::StatusCode, Filter, Rejection, Reply};

pub async fn start_rest_api(
    request_tx: mpsc::Sender<NodeApiRequest>,
    response_rx: mpsc::Receiver<NodeApiResponse>,
    rest_port: u16,
) {
    // Define the routes
    let value_route = value_fetch_filter(request_tx.clone(), response_rx.clone());
    let add_key_route = add_key_filter(request_tx.clone(), response_rx.clone());

    let routes = value_route.or(add_key_route);
    warp::serve(routes).run(([0, 0, 0, 0], rest_port)).await;
}

fn value_fetch_filter(
    request_tx: mpsc::Sender<NodeApiRequest>,
    mut response_rx: mpsc::Receiver<NodeApiResponse>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("value" / String)
        .and(warp::get())
        .and_then(move |key: String| {
            async move {
                // Send request to the node
                request_tx
                    .send(NodeApiRequest::GetValue(key.clone().into_bytes()))
                    .await
                    .unwrap();

                // Wait for the response
                match response_rx.recv().await {
                    Some(NodeApiResponse::ValueResult(result)) => match result {
                        Ok(value) => Ok(warp::reply::json(&json!({ "key": key, "value": value }))),
                        Err(err) => Ok(warp::reply::with_status(
                            warp::reply::json(&json!({ "error": err })),
                            StatusCode::INTERNAL_SERVER_ERROR,
                        )),
                    },
                    _ => Ok(warp::reply::with_status(
                        warp::reply::json(&json!({ "error": "Node response error" })),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    )),
                }
            }
        })
}

fn add_key_filter(
    request_tx: mpsc::Sender<NodeApiRequest>,
    mut response_rx: mpsc::Receiver<NodeApiResponse>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("add-key")
        .and(warp::post())
        .and(warp::body::json::<(String, Vec<u8>)>())
        .and_then(move |(key, value): (String, Vec<u8>)| {
            let request_tx = request_tx.clone();
            let response_rx = response_rx.clone();
            async move {
                // Send request to the node
                request_tx
                    .send(NodeApiRequest::AddNewValue(key.clone().into_bytes(), value))
                    .await
                    .unwrap();

                // Wait for the response
                match response_rx.recv().await {
                    Some(NodeApiResponse::AddValueResult(result)) => match result {
                        Ok(()) => Ok(warp::reply::json(
                            &json!({ "message": "Key added successfully" }),
                        )),
                        Err(err) => Ok(warp::reply::with_status(
                            warp::reply::json(&json!({ "error": err })),
                            StatusCode::INTERNAL_SERVER_ERROR,
                        )),
                    },
                    _ => Ok(warp::reply::with_status(
                        warp::reply::json(&json!({ "error": "Node response error" })),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    )),
                }
            }
        })
}
