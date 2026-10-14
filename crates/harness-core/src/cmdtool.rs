//! Local command tools: run a configured argv (never a shell string) and
//! return combined output as the tool result.

use crate::api::ToolDef;
use crate::config::CommandTool;
use serde_json::Value;
use std::time::Duration;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Execute a command tool. `arguments` is the raw JSON string produced by the
/// model; `{{key}}` placeholders in args are filled from it.
///
/// The child gets a minimal environment — never this app's own secrets.
pub async fn run(tool: &CommandTool, arguments: &str) -> Result<String, crate::error::Error> {
    let parsed: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
    let mut argv: Vec<String> = vec![tool.command.clone()];
    for arg in &tool.args {
        argv.push(substitute(arg, &parsed));
    }
    let output = tokio::time::timeout(DEFAULT_TIMEOUT, async {
        tokio::process::Command::new(&argv[0])
            .args(&argv[1..])
            .env_clear()
            .envs(crate::child_env().iter().map(|(k, v)| (k.clone(), v.clone())))
            .output()
            .await
    })
    .await
    .map_err(|_| crate::error::Error::Invalid("command timed out".into()))??;

    let mut out = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.stderr.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("[stderr]\n");
        out.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    if !output.status.success() {
        out.push_str(&format!(
            "\n[exit code: {}]",
            output.status.code().unwrap_or(-1)
        ));
    }
    Ok(out)
}

/// Replace `{{key}}` placeholders with values from the model's JSON arguments.
fn substitute(arg: &str, parsed: &Value) -> String {
    let mut result = arg.to_string();
    if let Value::Object(map) = parsed {
        for (k, v) in map {
            let placeholder = format!("{{{{{k}}}}}");
            if result.contains(&placeholder) {
                let value_str = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                result = result.replace(&placeholder, &value_str);
            }
        }
    }
    result
}

pub fn tool_def(name: &str, tool: &CommandTool) -> ToolDef {
    // An empty or partial `[cmd.x] schema` table must still reach the API as a
    // JSON Schema object, or the whole request is rejected.
    let parameters = crate::api::normalize_parameters(
        serde_json::to_value(&tool.schema).unwrap_or(serde_json::Value::Null),
    );
    ToolDef {
        name: format!("cmd_{name}"),
        description: tool.description.clone(),
        parameters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo_tool() -> CommandTool {
        CommandTool {
            command: "/bin/echo".into(),
            args: vec!["{{text}}".into()],
            description: "echo".into(),
            schema: toml::Table::new(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn runs_command_with_substitution() {
        let out = run(&echo_tool(), r#"{"text":"hello world"}"#)
            .await
            .unwrap();
        assert_eq!(out.trim(), "hello world");
    }

    #[test]
    fn substitutes_strings_and_numbers() {
        let parsed: Value = serde_json::from_str(r#"{"a":"x","b":3}"#).unwrap();
        assert_eq!(substitute("{{a}}-{{b}}", &parsed), "x-3");
        assert_eq!(substitute("no-placeholders", &parsed), "no-placeholders");
    }

    #[test]
    fn tool_def_gets_cmd_prefix() {
        let def = tool_def("echo", &echo_tool());
        assert_eq!(def.name, "cmd_echo");
    }

    #[tokio::test]
    async fn reports_nonzero_exit() {
        let tool = CommandTool {
            command: "/bin/false".into(),
            args: vec![],
            description: "fail".into(),
            schema: toml::Table::new(),
            enabled: true,
        };
        let out = run(&tool, "{}").await.unwrap();
        assert!(out.contains("[exit code: 1]"));
    }

    /// The parent's secrets must never reach a tool subprocess.
    #[tokio::test]
    async fn children_do_not_inherit_parent_secrets() {
        let _lock = crate::env_lock::guard();
        std::env::set_var("AURA_FAKE_SECRET", "sk-should-not-leak");

        let tool = CommandTool {
            command: "/usr/bin/env".into(),
            args: vec![],
            description: "print environment".into(),
            schema: toml::Table::new(),
            enabled: true,
        };
        let out = run(&tool, "{}").await.unwrap();
        assert!(
            !out.contains("AURA_FAKE_SECRET"),
            "tool subprocess saw a parent secret:\n{out}"
        );
        // …but it still gets what it needs to run.
        assert!(out.contains("PATH="), "PATH must survive env_clear");

        std::env::remove_var("AURA_FAKE_SECRET");
    }
}
