use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::transport::stdio;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::core::Invoker;
use crate::error::{Error, Result};
use crate::jasper::rules;

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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyArguments {
    provider: String,
    #[serde(default)]
    target: Option<String>,
}

#[derive(Debug, Serialize)]
struct PolicySnapshot {
    provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    minimum_accept_tokens: usize,
    accept: Vec<String>,
    deny: Vec<String>,
    ignored_accept: Vec<String>,
    unmatched: &'static str,
}

impl ToriiServer {
    pub fn new(invoker: Invoker) -> Self {
        Self { invoker }
    }

    fn tools(&self) -> Vec<Tool> {
        let mut tools = self
            .invoker
            .registry()
            .providers()
            .map(|provider| {
                Tool::new(
                    provider.config.tool.clone(),
                    provider.config.description.clone(),
                    Arc::new(tool_schema(provider)),
                )
            })
            .collect::<Vec<_>>();
        tools.push(policy_tool());
        tools
    }

    fn policy_snapshot(&self, arguments: PolicyArguments) -> Result<PolicySnapshot> {
        let provider = self
            .invoker
            .registry()
            .get(&arguments.provider)
            .ok_or_else(|| Error::ProviderNotFound(arguments.provider.clone()))?;
        let (target, rules_path) = if provider.uses_targets() {
            let target_name = arguments.target.as_deref().ok_or_else(|| {
                Error::InvalidArguments(format!(
                    "target is required for provider tool {:?}",
                    provider.config.tool
                ))
            })?;
            let target = provider.target(target_name).ok_or_else(|| {
                let available = provider.target_names().collect::<Vec<_>>().join(", ");
                Error::InvalidArguments(format!(
                    "unknown target {target_name:?} for provider tool {:?}; available: [{available}]",
                    provider.config.tool
                ))
            })?;
            let target_rules = target.paths.rules();
            let rules_path = if target_rules.exists() {
                target_rules
            } else {
                provider.paths.rules()
            };
            (Some(target.config.name.clone()), rules_path)
        } else {
            if arguments.target.is_some() {
                return Err(Error::InvalidArguments(format!(
                    "provider tool {:?} does not accept a target",
                    provider.config.tool
                )));
            }
            (None, provider.paths.rules())
        };
        let policy = rules::load(&rules_path)?;
        let ignored_accept = policy
            .invalid_accepts(provider.config.policy.minimum_accept_tokens)
            .into_iter()
            .map(str::to_owned)
            .collect();
        Ok(PolicySnapshot {
            provider: provider.config.name.clone(),
            target,
            minimum_accept_tokens: provider.config.policy.minimum_accept_tokens,
            accept: policy.accept,
            deny: policy.deny,
            ignored_accept,
            unmatched: "Arguments that match neither list are default-deny unless a human approves them or an active temporary grant matches.",
        })
    }
}

impl ServerHandler for ToriiServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new("torii", env!("CARGO_PKG_VERSION")))
            .with_instructions("Use Torii MCP tools for installed provider CLIs; never invoke those provider executables directly through a shell or try to bypass a denial. Before choosing an operation, call torii_policy with the provider tool and, when required, its announced target to inspect accept and deny rules. Pass argv as an array of strings. For target-aware tools, choose only a target announced by the schema. An announced alias is configured, not necessarily authorized: Torii asks the human before an inactive target can be used. If target access is denied, do not retry with a different alias. Multiple aliases may be temporarily active only when the human explicitly allows that. Policy remains target-local, default-deny, and explicit deny always wins. For an allowed call with managed authentication, Torii asks the human to authenticate automatically when the session is unavailable. There is no MCP reauth or target-management tool: a human uses the Torii control-plane CLI outside MCP. An aws_profile target is different: if its configured AWS CLI profile is unavailable or the active account does not match, ask a human to authenticate that configured profile through the native AWS CLI flow and retry; never choose a different alias or profile yourself.")
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
        if request.name == "torii_policy" {
            let value = Value::Object(request.arguments.unwrap_or_default());
            let arguments: PolicyArguments = serde_json::from_value(value).map_err(|error| {
                McpError::invalid_params(
                    format!("expected {{ provider: string, target?: string }}: {error}"),
                    None,
                )
            })?;
            return self
                .policy_snapshot(arguments)
                .map(|snapshot| CallToolResult::structured(json!(snapshot)))
                .map_err(|error| McpError::invalid_params(safe_error(&error), None));
        }
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

fn policy_tool() -> Tool {
    Tool::new(
        "torii_policy",
        "Read the active accept and deny rules for an installed Torii provider and optional target. This never executes a provider, reads credentials, or changes policy.",
        Arc::new(json!({
            "type": "object",
            "required": ["provider"],
            "properties": {
                "provider": { "type": "string", "description": "Installed provider MCP tool name." },
                "target": { "type": "string", "description": "Required for target-aware providers." }
            },
            "additionalProperties": false
        })
        .as_object()
        .cloned()
        .expect("static schema is an object")),
    )
    .with_annotations(ToolAnnotations::new().read_only(true))
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
    use crate::config::{ConfigPaths, Settings};
    use crate::providers::ProviderRegistry;

    #[test]
    fn targeted_tool_schema_announces_a_configured_alias_even_when_inactive() {
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
        let auth = paths.provider("auth");
        auth.ensure().unwrap();
        std::fs::write(
            auth.config(),
            r#"
version: "1"
name: auth
tool: auth
description: test authentication provider
command: auth
auth:
  strategy: inherited
  validate: { command: auth, args: [validate] }
"#,
        )
        .unwrap();
        let target = provider.target("mpce_dev");
        target.ensure().unwrap();
        std::fs::write(
            target.config(),
            "version: '1'\nname: mpce_dev\ncontext: local-context\nidentity:\n  provider: auth\n",
        )
        .unwrap();
        std::fs::write(
            provider.rules(),
            "version: '1.0'\ndeny: []\naccept: ['get shared']\n",
        )
        .unwrap();
        std::fs::write(
            target.rules(),
            "version: '1.0'\ndeny: ['get secret']\naccept: ['get target']\n",
        )
        .unwrap();

        let registry = ProviderRegistry::load(&paths).unwrap();
        let provider = registry.get("kubectl").unwrap();
        assert!(!provider.paths.target_authorizations().exists());
        let schema = tool_schema(&provider);

        assert_eq!(schema["required"], json!(["target", "args"]));
        assert_eq!(schema["properties"]["target"]["enum"], json!(["mpce_dev"]));

        let server = ToriiServer::new(Invoker::new(paths, Settings::default(), registry));
        let policy = server
            .policy_snapshot(PolicyArguments {
                provider: "kubectl".into(),
                target: Some("mpce_dev".into()),
            })
            .unwrap();
        assert_eq!(policy.accept, ["get target"]);
        assert_eq!(policy.deny, ["get secret"]);
    }

    #[test]
    fn aws_profile_schema_exposes_only_the_human_alias() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());
        let provider = paths.provider("aws-profile");
        provider.ensure().unwrap();
        std::fs::write(
            provider.config(),
            "version: '1'\nname: aws-profile\ntool: aws_profile\ndescription: test\ncommand: aws\ntargeting: { mode: aws_profile }\nauth: { strategy: inherited, validate: { command: aws, args: [] }, identity: { command: aws, args: [], field: Account }, profile_env: AWS_PROFILE }\n",
        )
        .unwrap();
        std::fs::write(provider.rules(), "version: '1.0'\ndeny: []\naccept: []\n").unwrap();
        let target = provider.target("prod");
        target.ensure().unwrap();
        std::fs::write(
            target.config(),
            "version: '1'\nname: prod\nidentity:\n  provider: aws_profile\n  scope: production-sso\n  profile: production-sso\n  expect: '123456789012'\n",
        )
        .unwrap();

        let registry = ProviderRegistry::load(&paths).unwrap();
        let provider = registry.get("aws_profile").unwrap();
        let schema = tool_schema(&provider);
        let serialized = serde_json::to_string(&schema).unwrap();

        assert_eq!(schema["properties"]["target"]["enum"], json!(["prod"]));
        assert!(!serialized.contains("production-sso"));
        assert!(!serialized.contains("123456789012"));
    }
}
