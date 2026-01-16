# Local Deployment of SmallThinker

SmallThinker can run fast on-device with just a CPU. Yushi supports deploying SmallThinker-4B-A0.6B.  
**SmallThinker is not optimized for function-calling; it does not treat a tool's return as a "task-finished" signal and may repeatedly invoke the same tool.**

1. Download the model  
   Grab `SmallThinker-4B-A0.6B-Instruct.Q4_K.gguf` from HugeFace or [ModelScope](https://modelscope.cn/models/PowerInfer/SmallThinker-4BA0.6B-Instruct-GGUF/files) and place it in `yushi/model/`.

2. Create the project  
   ```bash
   yushi new my_agent --localmodel smallthinker
   ```  
   This scaffolds a project named `my_agent` and adds the smallthinker feature plus Debian dependencies to `Cargo.toml`:

   ```toml
   [dependencies]
   app = { workspace = true, features = ["model-smallthinker"] }

   [package.metadata.deb]
   assets = [
       ["target/release/rk3576", "usr/bin/", "755"],
       ["assets/model/smallthinker", "usr/bin/", "755"],
   ]
   depends = "libaio1"
   ```

3. Build  
   ```bash
   yushi build --release --model --target aarch64-unknown-linux-musl
   ```  
   Produces two `.deb` packages inside `target/debian`: the agent and the model.  
   The `--model` flag tells the builder to package the model as well.

4. Install & run  
   ```bash
   sudo dpkg -i YOUR_AGENT_DEB_PACKAGE
   sudo dpkg -i YOUR_MODEL_DEB_PACKAGE
   sudo YOUR_AGENT_EXECUTABLE
   ```  
   Once started, open http://127.0.0.1:22786 to use the local SmallThinker model.