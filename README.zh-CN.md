[English](./README.md) | 简体中文

# Yushi是什么

Yushi（玉石）是一个无代码端侧智能体开发平台。

完全掌控，无限扩展。

只需几分钟，你就能在本地设备上运行自己的智能体。

# 与众不同之处

## 1. AI驱动，智能组装
## 2. 广义可逆计算，差量演进
## 3. 一切皆工具，无限扩展
## 4. 所见即所得，极易上手

# 功能
- [本地部署smallthinker](docs/zh-CN/smallthinker.zh-CN.md)，支持RK3588和RK3576。
以下命令会在`target/debian`生成rk3588和模型文件的deb包。
```bash
 yushi new rk3588 --localmodel smallthinker
 yushi build --release --model --target aarch64-unknown-linux-musl
```

# 快速开始

## 源码安装

**先决条件**
- 安装了Git、Rust
- 如未安装可以参考[先决条件](docs/zh-CN/prerequisites.zh-CN.md)进行安装

### 1. 将Yushi下载到本地
```sh
git clone https://github.com/yushi-apps/yushi.git
```

### 2. 切换目录
```sh
cd yushi
```

### 3. 安装Yushi
```sh
cargo install --path cli
```

### 4. 创建智能体
```sh
cargo yushi new my_agent
```
一个智能体的目录结构如下：
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
一个智能体就是一个普通的`Cargo`项目，带有`.yushi`配置文件。
- `main_prompt.md`是[智能体的提示（Prompt）文件](docs/zh-CN/mainagent.zh-CN.md)，使用Markdown格式编写。
- `model.toml`是智能体的模型配置文件
- `agents/`目录下存放的是[子代理](docs/zh-CN/subagent.zh-CN.md)，使用Markdowng格式编写，每个文件都是一个子智能体。


### 5. 配置智能体
```sh
cd my_agent
```
进入智能体目录，编辑`.yushi/model.toml`配置智能体使用的模型参数，默认使用`DeepSeek`，您只需填入`api_key`即可工作。

编辑`.yushi/main_prompt.md`定义智能体，如果文件为空则表示直接使用LLM模型。


### 6. 编译并运行
```sh
cargo yushi build --release
./my_agent/target/release/my_agent
```

**交叉编译请参考[交叉编译文档](docs/zh-CN/cross.zh-CN.md)。**

### 7. 使用
```sh
curl -X POST http://127.0.0.1:22786/chat -d "你具备哪些能力？"
```

# 社区

## 许可证
本项目采用双许可证模式。
- [AGPL](./LICENSE-AGPL.txt)
- 商业许可证请联系`yushi_app@163.com`获取详情。

## 示例
- [乐高Mindstorms EV3智能体](docs/ev3.md)

## 联系我们
- 邮箱：yushi_app@163.com
- 微信群：