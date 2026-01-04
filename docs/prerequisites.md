# Prerequisites
`Yushi` is built with `Rust`. To begin building agents with `Yushi`, you first need to install several dependencies.

## Linux or macOS

Install [Rust](https://rustup.rs/) with the following command via `rustup`:
```sh
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

## Windows

Visit https://rust-lang.net.cn/tools/install to install `rustup`.

Alternatively, you can use `winget` to install rustup using the following command in PowerShell:

``` sh
winget install --id Rustlang.Rustup
```
Be sure to restart your Terminal (and in some cases your system) for the changes to take affect.

## Configure for On-device Targets

Add the target with `rustup`
```sh
rustup target add  aarch64-unknown-linux-gnu
```