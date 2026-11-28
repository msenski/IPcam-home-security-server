use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::camera::Camera;
use crate::config::Config;
use crate::webrtc_handler;

use ::webrtc::peer_connection::RTCSessionDescription;
use axum::extract::{Json as ExtractJson, Path as ExtractPath, State};
use axum::http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use axum::{
    Json, Router, middleware,
    routing::{get, post},
};
use tower_http::services::ServeDir;

const STATIC_DIR: &str = "static";
const HLS_DIR: &str = "hls";

#[derive(serde::Deserialize)]
struct PtzMoveRequest {
    pan: f32,
    tilt: f32,
}

async fn no_cache(response: axum::response::Response) -> axum::response::Response {
    let mut response = response;
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store"),
    );
    response
}

/// Builds and runs the HTTP server on port 3000.
///
/// Serves static files from the default static directory, mounts each camera's
/// HLS output under `/hls/<camera_name>`, and exposes `/cameras` as a JSON list
/// of camera names.
pub async fn serve(
    config: &Config,
    cameras: Arc<HashMap<String, Arc<Camera>>>,
) -> anyhow::Result<()> {
    let names: Vec<String> = config
        .cameras
        .iter()
        .map(|cam_cfg| cam_cfg.name.clone())
        .collect();

    let mut router = Router::new()
        .route("/cameras", get(move || async move { Json(names) }))
        .fallback_service(ServeDir::new(STATIC_DIR));

    // Add camera-specific paths for the HLS files
    for cam_cfg in config.cameras.iter() {
        let hls_path = format!("/hls/{}", cam_cfg.name);
        let fs_path = PathBuf::from(HLS_DIR).join(&cam_cfg.name);
        router = router.nest_service(&hls_path, ServeDir::new(fs_path));
    }

    router = router
        // Add camera-specific paths for the WebRTC negotiation
        .route("/webrtc/{name}/offer", post(handle_webrtc_offer))
        // Add camera-specific PTZ handling routes
        .route("/cameras/{name}/ptz/move", post(handle_ptz_move))
        .route("/cameras/{name}/ptz/stop", post(handle_ptz_stop));

    let router: Router<()> = router
        .with_state(cameras)
        .layer(middleware::map_response(no_cache));

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.server_port)).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

async fn handle_webrtc_offer(
    State(cameras): State<Arc<HashMap<String, Arc<Camera>>>>,
    ExtractPath(name): ExtractPath<String>,
    ExtractJson(offer): ExtractJson<RTCSessionDescription>,
) -> std::result::Result<Json<RTCSessionDescription>, StatusCode> {
    // Check if camera exists
    let Some(_) = cameras.get(&name) else {
        return Err(StatusCode::NOT_FOUND);
    };

    // Try to create a PeerConnection and offer
    let pc = match webrtc_handler::create_peer_connection().await {
        Ok(pc) => pc,
        Err(err) => {
            eprint!("Failed to create WebRTC connection for {name}: {err:#}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Send back offer to browser on success
    match webrtc_handler::answer_offer(&pc, offer).await {
        Ok(answer_offer) => Ok(Json(answer_offer)),
        Err(err) => {
            eprint!("Failed to anser SDP offer for camera {name}: {err:#}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_ptz_move(
    State(cameras): State<Arc<HashMap<String, Arc<Camera>>>>,
    ExtractPath(name): ExtractPath<String>,
    ExtractJson(body): ExtractJson<PtzMoveRequest>,
) -> StatusCode {
    let Some(camera) = cameras.get(&name) else {
        return StatusCode::NOT_FOUND;
    };
    match camera.ptz_move(body.pan, body.tilt).await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn handle_ptz_stop(
    State(cameras): State<Arc<HashMap<String, Arc<Camera>>>>,
    ExtractPath(name): ExtractPath<String>,
) -> StatusCode {
    let Some(camera) = cameras.get(&name) else {
        return StatusCode::NOT_FOUND;
    };
    match camera.ptz_stop().await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
