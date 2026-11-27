//! Domain-agnostic CLI output, IPC request/response, and MCP stdio server helpers.

pub mod mcp;
pub mod output;

pub use mcp::{McpServerConfig, McpToolDef, mcp_ipc_result, mcp_text_result, serve_mcp_stdio};
pub use output::{
    dispatch_and_print, next_req_id, parse_envelope, print_ipc_response, print_json,
    query_and_print,
};
