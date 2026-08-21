use std::sync::Arc;

use super::{ConvertToAPITypeError, RequestParams, ResponseStream};
use crate::server::server_api::ServerApi;

pub async fn generate_multi_agent_output(
    server_api: Arc<ServerApi>,
    params: RequestParams,
    cancellation_rx: futures::channel::oneshot::Receiver<()>,
) -> Result<ResponseStream, ConvertToAPITypeError> {
    let _ = (server_api, params, cancellation_rx);
    Err(ConvertToAPITypeError::Unimplemented(
        "Warp Agent is excluded from this product profile".to_string(),
    ))
}
