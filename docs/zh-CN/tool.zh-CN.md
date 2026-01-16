# 工具

智能体通过工具连接物理世界，与现实世界进行交互。

## 什么是工具

工具通常是一个单一、原子化的操作或功能函数。每个工具：

- 拥有一个可执行的函数
- 包含自描述文档
- 具备结构化的输入/输出

工具完成注册后，智能体会根据工具的描述文档自主判断需要调用哪个工具。

## 可用工具
Yushi提供了一系列内置工具，您只需注册即可使用。

```sh
cd my_agent
yushi tool add <tool_name>
```

|名称|描述|
|------|-------|
|FileReadTool|读取文件内容|

## 快速开始

以下命令会创建一个自定义工具：

```sh
cd yushi
yushi tool new <tool_name>
```
命令执行成功后会在`yushi/tools`目录下创建一个名为`tool_name`的`Cargo`项目。

### 工具代码

工具是一个实现`Tool` trait的Rust结构体。
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

- `name`: 工具名称，全局唯一。
- `input_json_schema`: 工具输入参数的JSON Schema。
- `description`: 说明工具能做什么，让LLM了解工具的用途。
- `prompt`: 工具的详细使用手册，让LLM了解怎么使用工具。

### 工具注册
完成工具代码编写后，使用命令添加工具后就可以使用了。
```sh
cd my_agent
yushi tool add <tool_name>
`