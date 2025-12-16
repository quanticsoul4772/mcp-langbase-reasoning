use serde_json::Value;
use tracing::info;

use super::SharedState;
use crate::error::{McpError, McpResult};
use crate::modes::LinearParams;

/// Route tool calls to appropriate handlers
pub async fn handle_tool_call(
    state: &SharedState,
    tool_name: &str,
    arguments: Option<Value>,
) -> McpResult<Value> {
    info!(tool = %tool_name, "Routing tool call");

    match tool_name {
        "reasoning.linear" => handle_linear(state, arguments).await,
        _ => Err(McpError::UnknownTool {
            tool_name: tool_name.to_string(),
        }),
    }
}

/// Handle reasoning.linear tool call
async fn handle_linear(state: &SharedState, arguments: Option<Value>) -> McpResult<Value> {
    let params: LinearParams = match arguments {
        Some(args) => serde_json::from_value(args).map_err(|e| McpError::InvalidParameters {
            tool_name: "reasoning.linear".to_string(),
            message: e.to_string(),
        })?,
        None => {
            return Err(McpError::InvalidParameters {
                tool_name: "reasoning.linear".to_string(),
                message: "Missing arguments".to_string(),
            });
        }
    };

    let result =
        state
            .linear_mode
            .process(params)
            .await
            .map_err(|e| McpError::ExecutionFailed {
                message: e.to_string(),
            })?;

    serde_json::to_value(result).map_err(McpError::Json)
}
