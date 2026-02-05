use std::fs;
use std::path::Path;
use async_trait::async_trait;
use jieyusha::messages::{Message, ProgressMessage, ToolMessage, AssistantMessage};
use serde::Serialize;
use jieyusha::{JieyushaError, Result};
use jieyusha::{Tool, ToolUseContext, ToolResult};
use futures::stream::BoxStream;
use futures::stream::{self, StreamExt};

pub struct FileReadTool;

const IMAGE_EXTENSIONS: &[&str] = &[".png", ".jpg", ".jpeg", ".gif", ".bmp", ".tiff", ".svg", ".webp"];

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "FileRead"
    }

    fn description(&self) ->  &str {
        "Read a file from the local filesystem."
    }

    async fn prompt(&self) -> String {
        let max_line_to_read = 2000;
        let max_line_length = 2000;

        format!(
            r#"Reads a file from the local filesystem. 
                The file_path parameter must be an absolute path, not a relative path.
                By default, it reads up to {} lines starting from the beginning of the file. 
                You can optionally specify a line offset and limit (especially handy for long files), but it's recommended to read the whole file by not providing these parameters. 
                Any lines longer than {} characters will be truncated. For image files, the tool will return base64 string."#,
                max_line_to_read, max_line_length
        )
    }

    fn input_json_schema(&self) ->  &str {
        r#"
        {
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "The line number to start reading from. Only provide if the file is too large to read at once"
                },
                "limit": {
                    "type": "integer",
                    "description": "The number of lines to read. Only provide if the file is too large to read at once"
                }
            },
            "required": ["file_path"]
        }
        "#
    }

    async fn call(&self, input_data: &serde_json::Value, context: &ToolUseContext) -> ToolResult {
            //println!("FileReadTool: Reading file {}", file_path);
            //yield ToolMessage::progress("Read file", &context.tool_use_id);

            let tool_use_id = context.tool_use_id.clone();
            let file_path = match input_data.get("file_path").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => return ToolResult::error(
                    "missing or non-string file_path", 
                    &context.tool_use_id)
                //{
                //    yield Message::Tool(ToolMessage::from_error(
                //        &format!("missing or non-string file path {input_data}"), 
                //        &context.tool_use_id));
                //    return;
               // }
            };
            
            let offset = input_data.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let limit = match input_data.get("limit") {
                Some(o) => o.as_u64().map(|v| v as usize),
                None => None,
            };

            let ext = Path::new(&file_path)
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_lowercase();

            if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                return ToolResult::error(
                    "Image file reading not implemented yet", 
                    &context.tool_use_id);
                //yield Message::Tool(ToolMessage::from_error(
                //    "Image file reading not implemented yet", 
                //    &context.tool_use_id));
                //return;
            }

        let stream = async_stream::stream! {
            yield Message::Progress(ProgressMessage {
                r#type: "progress".to_string(),
                content: AssistantMessage::new(&format!("Reading text file: {}", file_path)),
                tools: None,
                tool_use_id: Some(tool_use_id.clone()),
            });
            match Self::read_text_content(&file_path, offset, limit) {
                Ok(text_data) => {
                    let content = serde_json::to_string(&text_data).unwrap();
                    yield Message::Tool(ToolMessage::new_content(
                        &content, 
                        &tool_use_id));
                }
                Err(e) => {
                    yield Message::Tool(ToolMessage::from_error(
                        &format!("Error reading file: {}", e), 
                        &tool_use_id));
                }
            }
        };
        ToolResult::new(Box::pin(stream))
    }
}

#[derive(Serialize)]
struct TextData {
    r#type: String,
    file_path: String,
    content: String,
    line_count: usize,
    start_line: usize,
    total_lines: usize
} 

impl FileReadTool {
    fn read_text_content(file_path: &str, offset: usize, max_lines: Option<usize>) -> Result<TextData> {
        //println!("Reading text file: {}, offset: {}, max_lines: {:?}", file_path, offset, max_lines);
        let content = fs::read_to_string(file_path)?;
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let end_index = if let Some(max) = max_lines {
            std::cmp::min(offset + max, total_lines)
        } else {
            total_lines
        };

        let start_index = std::cmp::min(offset, total_lines);
        let selected_lines = &lines[start_index..end_index];
        let line_count = selected_lines.len();
        let result_content = selected_lines.join("\n");
        Ok(TextData {
            r#type: "text".to_string(),
            file_path: file_path.to_string(),
            content: result_content,
            line_count: line_count,
            start_line: start_index,
            total_lines,
        })
    }
}