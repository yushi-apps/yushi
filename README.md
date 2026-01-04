English | [简体中文](./README.zh-CN.md)

# What is Yushi

Yushi is a no-code on-device AI agent development platform.

Total control. Infinite extensibility.

Get your own agent running locally in minutes.

# Distinctive features

## 1. AI-powered, intelligently assembled
## 2. Generalized reversible computing, delta evolution.
## 3. Everything is a plugin, designed for extension.
## 4. What you see is what you get, incredibly easy to use


# Quick Start

## Install from Git Source

**Prerequisites**
- Git and Rust is installed
- If it’s not installed yet, refer to the [prerequisites](docs/prerequisites.md) to set it up.

### 1. Download Yushi to your local machine
```sh
git clone https://github.com/yushi-apps/yushi.git
```

### 2. Change directory
```sh
cd yushi
```

### 3. Install Yushi
```sh
cargo install --path cli
```

### 4. Create an agent
```sh
cargo yushi new my_agent
```
The directory structure of an agent is as follows:
```
.
├── Cargo.toml
├── src/
│   ├── main.js
├── .yushi/
│   ├── main_prompt.md
│   ├── model.toml
│   ├── agents/
```
An agent is simply a regular `Cargo` project with a `.yushi` configuration file.
- `main_prompt.md` is [Agent Prompt file](docs/mainagent.md)，write in Markdown format.
- `model. toml`is the model configuration file for the agent
- `agents/` directory contains [sub-agents](docs/subagent.md), each written in Markdown—one file per sub-agent.


### 5. Configure the agent
```sh
cd my_agent
```
Navigate to the agent directory and edit `.yushi/model.toml` to set the model parameters.  By default, DeepSeek is used—just fill in your `api_key` and you're ready to go.

Edit `.yushi/main_prompt.md` to define the agent; if the file is empty, the LLM model will be used directly.


### 6. Build and run
```sh
cargo yushi build --release
./my_agent/target/release/my_agent
```
After the build finishes, copy the executable `my_agent/target/release/my_agent` to your target device and run it.

**For cross-compilation, please refer to the [Cross-compilation Guide](https://github.com/cross-rs/cross/blob/main/docs/getting-started.md) .**

### 7. Use it
```sh
curl -X POST http://127.0.0.1:22786/chat -d "What can you do？"
```

# Community

## License
This project is offered under a dual-license model.
- [AGPL](./LICENSE-AGPL.txt) for community edition
- For commercial licensing, please contact `yushi_app@163.com` for details.

## Examples
- [LEGO Mindstorms EV3 AI Agent](docs/ev3.md)

## Contact us
- Email：`yushi_app@163.com`