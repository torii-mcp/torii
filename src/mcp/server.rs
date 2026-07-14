use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::stdio;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::core::Invoker;
use crate::error::{Error, Result};

#[derive(Clone)]
pub struct ToriiServer {
    invoker: Invoker,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolArguments {
    #[serde(default)]
    target: Option<String>,
    args: Vec<String>,
}

impl ToriiServer {
    pub fn new(invoker: Invoker) -> Self {
        Self { invoker }
    }

    fn tools(&self) -> Vec<Tool> {
        self.invoker
            .registry()
            .providers()
            .map(|provider| {
                Tool::new(
                    provider.config.tool.clone(),
                    provider.config.description.clone(),
                    Arc::new(tool_schema(provider)),
                )
            })
            .collect()
    }
}

impl ServerHandler for ToriiServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new("torii", env!("CARGO_PKG_VERSION")))
            .with_instructions("Execute provider CLIs through Torii. Pass argv as an array of strings; policy is default-deny and explicit deny always wins.")
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.tools()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools().into_iter().find(|tool| tool.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, McpError> {
        if self.invoker.registry().get(&request.name).is_none() {
            return Err(McpError::invalid_params(
                format!("unknown Torii provider tool {:?}", request.name),
                None,
            ));
        }
        let value = Value::Object(request.arguments.unwrap_or_default());
        let arguments: ToolArguments = serde_json::from_value(value).map_err(|error| {
            McpError::invalid_params(
                format!("expected {{ target?: string, args: string[] }}: {error}"),
                None,
            )
        })?;
        if arguments.args.is_empty() {
            return Err(McpError::invalid_params(
                "args must contain at least one string",
                None,
            ));
        }
        match self
            .invoker
            .invoke(&request.name, arguments.target.as_deref(), &arguments.args)
            .await
        {
            Ok(result) => {
                let value = serde_json::to_value(result)
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?;
                Ok(CallToolResult::structured(value))
            }
            Err(error) => Ok(CallToolResult::structured_error(json!({
                "provider": request.name,
                "error": safe_error(&error),
            }))),
        }
    }
}

pub async fn serve(invoker: Invoker) -> Result<()> {
    let service = ToriiServer::new(invoker)
        .serve(stdio())
        .await
        .map_err(|error| Error::Mcp(error.to_string()))?;
    service
        .waiting()
        .await
        .map_err(|error| Error::Mcp(error.to_string()))?;
    Ok(())
}

fn tool_schema(provider: &crate::providers::Provider) -> Map<String, Value> {
    let mut properties = Map::new();
    properties.insert(
        "args".into(),
        json!({ "type": "array", "items": { "type": "string" }, "minItems": 1 }),
    );
    let mut required = vec![Value::String("args".into())];
    if provider.uses_targets() {
        let names = provider.target_names().collect::<Vec<_>>();
        let target_schema = if names.is_empty() {
            json!({ "type": "string", "not": {} })
        } else {
            json!({ "type": "string", "enum": names })
        };
        properties.insert("target".into(), target_schema);
        required.insert(0, Value::String("target".into()));
    }
    json!({
        "type": "object",
        "required": required,
        "properties": properties,
        "additionalProperties": false
    })
    .as_object()
    .cloned()
    .expect("static schema is an object")
}

fn safe_error(error: &Error) -> String {
    // Error variants are deliberately designed without credential values or child output.
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigPaths;
    use crate::providers::ProviderRegistry;

    #[test]
    fn targeted_tool_schema_requires_a_loaded_alias() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());
        let provider = paths.provider("kubectl");
        provider.ensure().unwrap();
        std::fs::write(
            provider.config(),
            r#"
version: "1"
name: kubectl
tool: kubectl
description: test
command: kubectl
targeting: { mode: kubectl_context }
"#,
        )
        .unwrap();
        let target = provider.target("mpce_dev");
        target.ensure().unwrap();
        std::fs::write(
            target.config(),
            "version: '1'\nname: mpce_dev\ncontext: eks-mpce-dev\n",
        )
        .unwrap();

        let registry = ProviderRegistry::load(&paths).unwrap();
        let provider = registry.get("kubectl").unwrap();
        let schema = tool_schema(&provider);

        assert_eq!(schema["required"], json!(["target", "args"]));
        assert_eq!(schema["properties"]["target"]["enum"], json!(["mpce_dev"]));
    }
}
