# Tool

Agents connect to the physical world through tools and interact with reality.

## What is a tool

A tool is usually a single, atomic operation or utility function. Each tool:

- Has an executable function  
- Includes self-describing documentation  
- Provides structured input and output  

Once registered, the agent decides on its own which tool to call based on the tool’s description.

## Available tools
Yushi offers a series of built-in tools, and you can use them by simply registering.

```sh
cd my_agent
yushi tool add <tool_name>
```

|Name|Description|
|------|-------|
|FileReadTool|Read a file|

## Quick Start

The following command creates a custom tool.

```sh
cd yushi
yushi tool new <tool_name>
```
After the command runs successfully, a `Cargo` project named `tool_name` will be created under the `yushi/tools` directory.

### Tool code

A tool is a Rust struct that implements the `Tool` trait.
```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn input_json_schema(&self) -> &str;
    fn description(&self) -> &str;
    async fn prompt(&self) -> String;
    async fn call(&self, input_data: &serde_json::Value, context: &mut ToolUseContext) -> Result<ToolMessage>;
}
```
- `name`: the tool’s globally unique identifier  
- `input_json_schema`: JSON Schema defining the tool’s input parameters  
- `description`: explains what the tool does so the LLM knows when to use it  
- `prompt`: detailed manual for the tool so the LLM knows how to use it

### Tool Registration
Once you’ve finished writing the tool code, run the add-tool command and it’s ready to use.
```sh
cd my_agent
yushi tool add <tool_name>
```