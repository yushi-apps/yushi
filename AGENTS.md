# Repository Guidelines

## 项目结构和模块组织

- 子Agent：`agents/`，存储子Agent的Markdown描述文件
- 文档：`docs/`
- CLI：`app\`
- Agent引擎：`jieyusha/`
- 可逆计算引擎：`tuo`，定义文件在`tuo/xdefs/`，其中`xdsl.xdef`是差量合并操作定义文件，`memory.xdef`是LLM记忆定义文件，`task.xdef`是任务定义文件。
- 工具：`tools`，
- 技能：`skills`

## 构建和测试
- 采用Rust工作区方式管理
- 添加依赖：`cargo add`
- 安装可执行程序：`cargo install`
- 编译运行：`cargo run`
- 测试：`cargo test`
- 代码格式化：`cargo fmt`

## 编码规范和命名规则

- 语言：Rust，严格遵循官方规范。
- 使用rustfmt格式化代码。
- 为棘手或不明显的逻辑添加简短的代码注释。
- 保持文件简洁
- 尽量将文件控制在 700 行代码左右；**仅为指导原则，并非硬性门槛。**当拆分或重构有助于提升代码清晰度或可测试性时再进行。
- 命名规范：产品、应用及文档标题使用`Yushi`；CLI 命令、包名/二进制文件、路径及配置键名则使用`yushi`。