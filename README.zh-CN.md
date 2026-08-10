# MCP Doctor

[English](README.md) | [简体中文](README.zh-CN.md)

`mcp-doctor` 是一个本地、静态的 stdio MCP server 配置预检器。它帮助 AI
Agent 开发者定位常见的启动前故障：客户端 `PATH` 中找不到 `npx` 或 Node、
工作目录无效、环境变量占位符未解析。这些问题通常发生在 MCP Inspector 或
客户端真正启动 server 之前。

> 状态：Alpha（`v0.1.0` 范围）。本版本只有 CLI，不会启动配置中的命令，也不
> 会连接任何 MCP server。

## 为什么需要它

同类启动故障在真实报告中反复出现：MCP 官方 servers 的 [#40](https://github.com/modelcontextprotocol/servers/issues/40)、
[#64](https://github.com/modelcontextprotocol/servers/issues/64)、
[#447](https://github.com/modelcontextprotocol/servers/issues/447)，Cline 的
[#1948](https://github.com/cline/cline/issues/1948)、[#902](https://github.com/cline/cline/issues/902)，
Continue 的 [#4791](https://github.com/continuedev/continue/issues/4791)、
[#7509](https://github.com/continuedev/continue/issues/7509)，以及 GitHub MCP server 的
[#1396](https://github.com/github/github-mcp-server/issues/1396)。Stack Overflow 的
[spawn npx 问题](https://stackoverflow.com/questions/79534396/spawn-npx-enoent-spawn-npx-enoent-error-in-cline-vscode-mcp-server-connection)
也显示这不是单一客户端的问题。

## 安装

Rust 1.85 或更新版本从源码构建：

```bash
git clone https://github.com/Tinkora/mcp_doctor.git
cd mcp_doctor
cargo install --path . --locked
```

## 快速开始

显式检查一个配置文件：

```bash
mcp-doctor ~/.cursor/mcp.json
```

没有路径时，工具检查当前工作区以及当前用户已存在的 Claude Desktop、Cline、
Cursor 约定路径：

```bash
mcp-doctor
```

自动化使用稳定 JSON，并在静态检查出现错误时失败：

```bash
mcp-doctor --format json --ci .vscode/mcp.json
echo "$?" # 0 = 无错误，1 = 检查错误，2 = 输入错误
```

默认人类报告会列出 server、位置、诊断代码和简短修复提示，但不会打印配置中的
环境变量值。

## 支持的配置

MVP 只读取 JSON：

- 顶层为 `mcpServers` map（Claude Desktop、Cline 风格）；
- 顶层为 `servers` map（VS Code 风格）；
- stdio 字段 `command`，可选字符串数组 `args`、字符串 `cwd`、字符串 map `env`。

带 `url`、HTTP、SSE 或其他非 stdio `type` 的远程条目会报告不支持，不会发起连接。
在有独立需求和兼容性证据前，YAML、TOML、MCP catalog、协议握手和 server 执行均不在范围内。

## 安全与隐私

MCP Doctor 默认只读。它读取指定 JSON 和文件元数据，并读取进程 `PATH` 来检查裸命令
可发现性。它不会启动进程、发起网络请求、从进程环境中读取占位符对应的值，也不会在
报告中包含配置的环境变量值。占位符诊断可能出现环境变量名。分享配置前请先移除 secret。

## 与 MCP Inspector 的边界

[MCP Inspector](https://github.com/modelcontextprotocol/inspector) 是协议调试器，
可以启动 server、执行握手、检查 tools，并支持 stdio 和远程传输。MCP Doctor 是这个流程
之前的预检层：发现客户端配置、静态检查本地进程前提，并提供适合 CI 的 JSON 和退出码。
它不替代 Inspector，也不宣称 server 的 MCP 协议实现正确。

## 开发

```bash
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

测试覆盖支持的配置封装、命令/PATH 和 cwd 诊断、占位符脱敏、不支持的传输、坏输入、
CLI 输出、CI 退出码和“不执行命令”边界。

请阅读[产品规格](docs/PRODUCT_SPEC.zh-CN.md)了解证据门槛、发现路径和停止条件；修改前请阅读
[CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)、[SECURITY.zh-CN.md](SECURITY.zh-CN.md) 和
[CHANGELOG.md](CHANGELOG.md)。

## 许可证

[MIT](LICENSE) Copyright Tinkora contributors.
