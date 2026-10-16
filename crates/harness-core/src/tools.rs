//! Tool plumbing: providers contribute tool definitions and execute calls.

use crate::api::ToolDef;
use crate::builtins::BuiltinTool;
use crate::error::{Error, Result};
use std::collections::{BTreeMap, HashSet};

/// Where a tool call came from and how to run it.
#[derive(Debug, Clone)]
pub enum ToolExecutor {
    /// Built-in tool shipped with the harness (shell, read, write, ...).
    Builtin(BuiltinTool),
    /// Local command; requires user approval before each run.
    Command(crate::config::CommandTool),
    /// MCP tool: server name + original tool name on that server.
    Mcp { server: String, tool: String },
}

#[derive(Debug, Clone)]
pub struct RegisteredTool {
    pub def: ToolDef,
    pub executor: ToolExecutor,
}

/// Flattened registry of every enabled tool across providers.
#[derive(Debug, Clone, Default)]
pub struct ToolRegistry {
    by_name: BTreeMap<String, RegisteredTool>,
    /// Built-in names: plugins/MCP can add tools, never shadow the core ones.
    builtins: HashSet<String>,
    /// Full access mode: mutating tools run without an approval prompt.
    auto_approve: bool,
}

impl ToolRegistry {
    pub fn new(auto_approve: bool) -> Self {
        let mut reg = Self {
            auto_approve,
            ..Self::default()
        };
        for tool in BuiltinTool::all() {
            reg.builtins.insert(tool.name().into());
            reg.by_name.insert(
                tool.name().into(),
                RegisteredTool {
                    def: tool.def(),
                    executor: ToolExecutor::Builtin(tool),
                },
            );
        }
        reg
    }

    pub fn register(&mut self, tool: RegisteredTool) {
        if self.builtins.contains(&tool.def.name) {
            return; // built-ins are not overridable by plugins
        }
        self.by_name.insert(tool.def.name.clone(), tool);
    }

    pub fn definitions(&self) -> Vec<ToolDef> {
        self.by_name.values().map(|t| t.def.clone()).collect()
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    pub fn get(&self, name: &str) -> Result<&RegisteredTool> {
        self.by_name
            .get(name)
            .ok_or_else(|| Error::ToolNotFound(name.to_string()))
    }

    /// True when the tool needs an explicit user approval before running.
    pub fn needs_approval(&self, name: &str) -> Result<bool> {
        if self.auto_approve {
            return Ok(false);
        }
        Ok(match self.get(name)?.executor {
            ToolExecutor::Builtin(t) => t.needs_approval(),
            ToolExecutor::Command(_) => true,
            ToolExecutor::Mcp { .. } => false,
        })
    }
}

/// Build the registry from user plugin config. MCP tools are added later,
/// once servers connect (`mcp::collect_server_tools`).
pub fn from_plugins(plugins: &crate::config::Plugins, auto_approve: bool) -> ToolRegistry {
    let mut reg = ToolRegistry::new(auto_approve);
    for (name, tool) in &plugins.cmd {
        if tool.enabled {
            reg.register(RegisteredTool {
                def: crate::cmdtool::tool_def(name, tool),
                executor: ToolExecutor::Command(tool.clone()),
            });
        }
    }
    reg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CommandTool, Plugins};

    #[test]
    fn registry_collects_enabled_command_tools_only() {
        let mut plugins = Plugins::default();
        plugins.cmd.insert(
            "echo".into(),
            CommandTool {
                command: "/bin/echo".into(),
                args: vec![],
                description: "echo".into(),
                schema: toml::Table::new(),
                enabled: true,
            },
        );
        plugins.cmd.insert(
            "off".into(),
            CommandTool {
                command: "/bin/true".into(),
                args: vec![],
                description: "disabled".into(),
                schema: toml::Table::new(),
                enabled: false,
            },
        );
        let reg = from_plugins(&plugins, false);
        // 7 built-ins are always present, plus the one enabled command tool.
        assert_eq!(reg.len(), 8);
        assert!(reg.get("cmd_echo").is_ok());
        assert!(reg.get("shell").is_ok());
        assert!(matches!(
            reg.get("cmd_echo").unwrap().executor,
            ToolExecutor::Command(_)
        ));
        assert!(reg.needs_approval("cmd_echo").unwrap());
    }

    #[test]
    fn full_access_mode_skips_the_approval_gate() {
        let mut plugins = Plugins::default();
        plugins.cmd.insert(
            "echo".into(),
            CommandTool {
                command: "/bin/echo".into(),
                args: vec![],
                description: "echo".into(),
                schema: toml::Table::new(),
                enabled: true,
            },
        );
        let ask = from_plugins(&plugins, false);
        assert!(ask.needs_approval("shell").unwrap());
        assert!(ask.needs_approval("cmd_echo").unwrap());
        assert!(!ask.needs_approval("read_file").unwrap());

        let full = from_plugins(&plugins, true);
        assert!(!full.needs_approval("shell").unwrap());
        assert!(!full.needs_approval("cmd_echo").unwrap());
        assert!(!full.needs_approval("read_file").unwrap());
    }

    #[test]
    fn unknown_tool_errors() {
        let reg = ToolRegistry::new(false);
        assert!(matches!(
            reg.get("nope"),
            Err(Error::ToolNotFound(n)) if n == "nope"
        ));
    }

    #[test]
    fn mcp_registration_uses_prefixed_names() {
        let mut reg = ToolRegistry::new(false);
        reg.register(RegisteredTool {
            def: ToolDef {
                name: "mcp_mnemai_memory_search".into(),
                description: "search memory".into(),
                parameters: serde_json::json!({"type":"object"}),
            },
            executor: ToolExecutor::Mcp {
                server: "mnemai".into(),
                tool: "memory_search".into(),
            },
        });
        assert!(!reg.needs_approval("mcp_mnemai_memory_search").unwrap());
    }
}
