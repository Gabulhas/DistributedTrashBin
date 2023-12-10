use crate::network::node::GetValueResponse;
use crate::Node;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use warp::{Filter, Rejection, Reply};

pub async fn start_rest_api(node: Arc<Mutex<Node>>) {
    // Assuming you have a function to create a database

    // Define the routes
    let start_value_fetch_route = start_value_fetch_filter(node.clone());
    //let add_key_route = add_key_filter(node.clone());

    let add_key_filter_route = add_key_filter(node.clone());
    // Combine all routes
    let routes = start_value_fetch_route.or(add_key_filter_route);

    // Start the Warp server
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

fn start_value_fetch_filter(
    node: Arc<Mutex<Node>>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let node_clone = node.clone(); // Clone the Arc outside the closure

    warp::path!("value" / String)
        .and(warp::get())
        .and_then(move |key: String| {
            let node_clone_inner = node_clone.clone(); // Clone the Arc for each closure call
            async move {
                let mut node_clone = node_clone_inner.lock().await; // Now the lock is on the cloned Arc
                match node_clone.get_value(key.clone().into_bytes()).await {
                    Ok(GetValueResponse::Owner(actual_value)) => Ok::<_, Rejection>(
                        warp::reply::json(&json!({ "key": key, "value": actual_value })),
                    ),
                    Ok(GetValueResponse::Requested(state)) => {
                        Ok(warp::reply::json(&json!({ "key": key, "state": state })))
                    }
                    Err(err) => {
                        let error_message = format!("{}", err);
                        Ok(warp::reply::json(&json!({ "error": error_message })))
                    }
                }
            }
        })
}

fn add_key_filter(
    node: Arc<Mutex<Node>>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let node_clone = node.clone(); // Clone the Arc outside the closure

    warp::path!("add-key")
        .and(warp::post())
        .and(warp::body::json::<(String, Vec<u8>)>()) // Assuming a tuple is used for combined data
        .and_then(move |(key, value): (String, Vec<u8>)| {
            let node_clone_inner = node_clone.clone(); // Clone the Arc for each closure call
            async move {
                match node_clone_inner
                    .lock()
                    .await
                    .add_new_value(key.into_bytes(), value)
                    .await
                {
                    Ok(()) => Ok::<_, warp::Rejection>(warp::reply::with_status(
                        warp::reply::json(
                            &serde_json::json!({ "message": "Key added successfully" }),
                        ),
                        warp::http::StatusCode::OK,
                    )),
                    Err(err) => {
                        let error_message = serde_json::json!({ "error": format!("{}", err) });
                        Ok::<_, warp::Rejection>(warp::reply::with_status(
                            warp::reply::json(&error_message),
                            warp::http::StatusCode::BAD_REQUEST,
                        ))
                    }
                }
            }
        })
}
