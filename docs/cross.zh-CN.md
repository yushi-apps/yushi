# 交叉编译 

`Yushi使用[Cross](https://github.com/cross-rs/cross)进行交叉编译，可以参考[开始文档](https://github.com/cross-rs/cross/blob/main/docs/getting-started.md)进行安装设置。

## 编译

完成安装后使用`cross build --target <target>`编译目标可执行文件，如下编译`aarch64-unknown-linux-gnu`目标文件。

```bash
cross build --release --target aarch64-unknown-linux-gnu
```
编译完成后目标文件在`target/aarch64-unknown-linux-gnu/release/my_agent`

## 运行

把目标文件复制到目标设备，并运行。
```bash
scp target/aarch64-unknown-linux-gnu/release/my_agent user@device:/home/user/
ssh user@device ./my_agent
```