use std::collections::HashMap;
use std::process::Stdio;
use std::time::{Duration, Instant};
use async_trait::async_trait;
use tokio::process::Command;
use tokio::io::{BufReader, AsyncBufReadExt};
use jieyusha::messages::*;
use jieyusha::{Tool, ToolResult, ToolUseContext};

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        "Execute shell commands."
    }

    async fn prompt(&self) -> String {
        r#"Execute shell commands.

Parameters:
- cmd: The shell command to execute
- workdir: Working directory (optional, defaults to current directory)
- env: Environment variables (optional, key-value pairs)
- timeout: Timeout in seconds (optional)

Examples:
- Execute: { "cmd": "ls -la" }"#.to_string()
    }

    fn input_json_schema(&self) -> &str {
        r#"{
            "type": "object",
            "properties": {
                "cmd": {
                    "type": "string",
                    "description": "Shell command to execute"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory (defaults to current directory)"
                },
                "env": {
                    "type": "object",
                    "description": "Environment variables as key-value pairs",
                    "additionalProperties": {
                        "type": "string"
                    }
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (optional)"
                }
            }
        }"#
    }

    async fn call(&self, input_data: &serde_json::Value, context: &ToolUseContext) -> ToolResult {
        let tool_use_id = context.tool_use_id.clone();
        let command = match input_data.get("cmd").and_then(|v| v.as_str()) {
            Some(cmd) => cmd.to_string(),
            None => return ToolResult::error("command is required", &tool_use_id),
        };

        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if let Some(workdir) = input_data.get("workdir").and_then(|v| v.as_str()) {
            cmd.current_dir(workdir);
        };

        if let Some(env) = input_data
            .get("env")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect::<HashMap<String, String>>()
            }) {
                for (key, value) in &env {
                        cmd.env(key, value);
                }
            }

        let timeout_secs = input_data.get("timeout").and_then(|v| v.as_u64()).unwrap_or(60);

        match cmd.spawn() {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                
                let mut output = String::new();
                
                // Handle stdout
                if let Some(stdout_handle) = stdout {
                    let mut reader = BufReader::new(stdout_handle);
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line).await {
                            Ok(0) => break, // EOF
                            Ok(_) => {
                                output.push_str(line.as_str());
                            }
                            Err(_) => break,
                        }
                    }
                }

                // Handle stderr
                if let Some(stderr_handle) = stderr {
                    let mut reader = BufReader::new(stderr_handle);
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line).await {
                            Ok(0) => break, // EOF
                            Ok(_) => {
                                // Add stderr to output as well
                                output.push_str(line.as_str());
                            }
                            Err(_) => break,
                        }
                    }
                }
                
                let status = tokio::time::timeout(
                    Duration::from_secs(timeout_secs),
                    child.wait()
                ).await;
                
                match status {
                    Ok(result) => {
                        match result {
                            Ok(_) => {
                                let tool_message = Message::Tool(ToolMessage::new_content(output.trim().to_string(), &tool_use_id));
                                let messages = vec![tool_message];
                                
                                let stream = futures::stream::iter(messages);
                                ToolResult::new(Box::pin(stream))
                            }
                            Err(e) => {
                                ToolResult::error(format!("Command execution failed: {}", e), &tool_use_id)
                            }
                        }
                    }
                    Err(_) => {
                        let _ = child.kill().await;
                        ToolResult::error(format!("Command execution timed out ({} seconds)", timeout_secs), &tool_use_id)
                    }
                }
            }
            Err(e) => {
                ToolResult::error(format!("Failed to start command: {}", e), &tool_use_id)
            }
        }
    }
}