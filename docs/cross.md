# Cross Compilation

`Yushi` uses [Cross](https://github.com/cross-rs/cross) for cross-compilation; refer to the [Getting Started guide](https://github.com/cross-rs/cross/blob/main/docs/getting-started.md) for installation and setup.

## Compilation

After installation, compile the target binary with `cross build --target <TARGET>`.  
For example, to build for `aarch64-unknown-linux-gnu`:

```bash
cross build --release --target aarch64-unknown-linux-gnu
```

Once the build finishes, the binary will be located at  
`target/aarch64-unknown-linux-gnu/release/my_agent`.

## Running

Copy the binary to the target device and run it:

```bash
scp target/aarch64-unknown-linux-gnu/release/my_agent user@device:/home/user/
ssh user@device ./my_agent
```