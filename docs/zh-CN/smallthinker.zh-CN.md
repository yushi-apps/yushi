# 本地部署smallthinker

端侧CPU即可快速推理运行smallthinker模型，Yushi支持部署SmallThinker-4B-A0.6B。**Smallthinker没有针对`function-calling`进行优化，不会把 tool 返回结果当成“任务已结束”的信号，存在循环调用同一个工具的现象。**

1. 下载模型文件
在HugeFace或[ModelScope](https://modelscope.cn/models/PowerInfer/SmallThinker-4BA0.6B-Instruct-GGUF/files)上下载`SmallThinker-4B-A0.6B-Instruct.Q4_K.gguf`模型文件，并放到`yushi/model/`目录下。

2. 创建项目
``` bash
yushi new my_agent --localmodel smallthinker
```
该命令会创建一个名为`my_agent`的项目，并在`Cargo.toml`中添加`smallthinker`特性和deb包依赖。

``` toml
[dependencies]
app = {workspace = true, features = ["model-smallthinker"]}
[package.metadata.deb]
assets = [
    ["target/release/rk3576", "usr/bin/", "755"],
    ["assets/model/smallthinker", "usr/bin/", "755"],
]
depends = "libaio1"
```

3. 编译构建
```bash
yushi build --release --model --target aarch64-unknown-linux-musl
```
会构建生成`target/debian`目录下的`my_agent`和`yushi-model`模型deb包。参数`--model`说明是否构建模型deb包。

4. 安装运行
``` bash
sudo dpkg -i YOUR_AGENT_DEB_PACKAGE
sudo dpkg -i YOUR_MODEL_DEB_PACKAGE
sudo YOUR_AGENT_EXECUTABLE
```
运行成功后，访问`http://127.0.0.1:22786`即可使用本地SmallThinker模型。