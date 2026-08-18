# MCP Doctor

[English](README.md) | [简体中文](README.zh-CN.md)

<!-- markdownlint-disable MD033 -->
<p align="center">
  <a href="https://ko-fi.com/tinkora" target="_blank" rel="noopener noreferrer">
    <img
      src="https://ko-fi.com/img/githubbutton_sm.svg"
      alt="在 Ko-fi 上支持 Tinkora"
      width="520"
    >
  </a>
</p>
<!-- markdownlint-enable MD033 -->

`mcp-doctor` 是一个本地、静态的 stdio MCP server 配置预检器。它帮助 AI
Agent 开发者定位常见的启动前故障：客户端 `PATH` 中找不到 `npx` 或 Node、
工作目录无效、环境变量占位符未解析。这些问题通常发生在 MCP Inspector 或
客户端真正启动 server 之前。

> 状态：Alpha（`v0.1.6` 范围）。本版本只有 CLI，不会启动配置中的命令，也不
> 会连接任何 MCP server。

## 为什么需要它

同类启动故障在真实报告中反复出现：MCP 官方 servers 的 [#40](https://github.com/modelcontextprotocol/servers/issues/40)、
[#64](https://github.com/modelcontextprotocol/servers/issues/64)、
[#447](https://github.com/modelcontextprotocol/servers/issues/447)，Cline 的
[#1948](https://github.com/cline/cline/issues/1948)、[#902](https://github.com/cline/cline/issues/902)，
Continue 的 [#4791](https://github.com/continuedev/continue/issues/4791)、
[#7509](https://github.com/continuedev/continue/issues/7509)，以及 GitHub MCP
server 的
[#1396](https://github.com/github/github-mcp-server/issues/1396)。
GitHub Copilot CLI 的 [#3379](https://github.com/github/copilot-cli/issues/3379)
报告仓库级定义静默覆盖同名用户级 server；[#4478](https://github.com/github/copilot-cli/issues/4478)
报告仅大小写不同的名称会重复启动 MCP 进程。
Claude Code 的 [#54803](https://github.com/anthropics/claude-code/issues/54803)
报告 user scope 配置被写入但列表不读取的位置；
[#77325](https://github.com/anthropics/claude-code/issues/77325) 和
[#58919](https://github.com/anthropics/claude-code/issues/58919) 则显示
`~/.claude.json` 中的无效或嵌套条目会破坏 MCP 配置。
Codex 的 [#37616](https://github.com/openai/codex/issues/37616) 和
[#13464](https://github.com/openai/codex/issues/13464) 显示错误语法或重复 MCP table
会使 `config.toml` 无法使用；[#26011](https://github.com/openai/codex/issues/26011)
和 [#33104](https://github.com/openai/codex/issues/33104) 则报告过期路径和找不到 MCP
命令。
Stack Overflow 的
[spawn npx 问题](https://stackoverflow.com/questions/79534396/spawn-npx-enoent-spawn-npx-enoent-error-in-cline-vscode-mcp-server-connection)
也显示这不是单一客户端的问题。

## 安装

Rust 1.85 或更新版本从源码构建：

```bash
git clone https://github.com/Tinkora/mcp_doctor.git
cd mcp_doctor
cargo install --path . --locked
```

带 tag 的 Release 还会提供 Linux x86-64、macOS Apple Silicon、macOS x86-64
和 Windows x86-64 预编译归档。每个归档都附带对应的 SHA-256 checksum。Release
还包含 CycloneDX 1.5 依赖 SBOM，以及构建来源和 SBOM attestation，可从
[Releases 页面](https://github.com/Tinkora/mcp_doctor/releases)下载。

## 快速开始

显式检查一个配置文件：

```bash
mcp-doctor ~/.cursor/mcp.json
```

没有路径时，工具检查当前工作区以及当前用户已存在的 Codex、Claude Code、Claude
Desktop、Cline、Cursor、VS Code 和 GitHub Copilot CLI 约定路径。仓库级发现包括
`.codex/config.toml`、`.devcontainer/devcontainer.json`、`.vscode/mcp.json`、
`.mcp.json`、`.github/mcp.json`、`.github/mcp-config.json` 和
`.cursor/mcp.json`；用户级发现包括 Codex 的 `~/.codex/config.toml`、Claude Code
的 `~/.claude.json`、Copilot CLI 的 `~/.copilot/mcp-config.json` 以及当前平台的
VS Code 用户级 `mcp.json`。对于 Claude Code，MCP Doctor 只检查顶层 user server
和属于当前工作区的 local server。

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

同时检查多个文件时，完全同名或仅大小写不同的 stdio server 会在每个受影响文件中
收到 `server_name_conflict` warning。不同客户端和版本的优先级并不一致，因此 MCP
Doctor 不会替客户端选择胜出定义。

### 如何理解路径检查

裸命令只根据启动 MCP Doctor 的当前环境检查。`path_context` warning 表示该命令在
当前环境中存在，但 GUI MCP 客户端可能继承不同的 `PATH`。为了保护配置值，检查不会
使用 `env.PATH`。Windows 上会使用当前 `PATHEXT`；缺失时回退到
`.COM;.EXE;.BAT;.CMD`。

绝对命令路径和绝对工作目录可以确定性检查。相对 `cwd` 或相对命令路径的解析基准
取决于客户端时，只报告 warning；如果 `cwd` 是绝对路径，则会据此检查相对命令。
这样不会假设 Claude Desktop、Cline、Cursor 和 VS Code 对所有相对路径采用相同语义。

## 支持的配置

MVP 读取 JSON、JSONC（允许注释与尾逗号）和 Codex TOML：

- 顶层为 `mcpServers` map（Claude Desktop、Cline 风格）；
- 顶层为 `servers` map（VS Code 风格）；
- `.devcontainer/devcontainer.json` 中的 VS Code Dev Container
  `customizations.vscode.mcp.servers` map（参见
  [VS Code 官方 MCP 文档](https://code.visualstudio.com/docs/agent-customization/mcp-servers)）；
- `~/.claude.json` 顶层 `mcpServers` 中的 Claude Code user server，以及
  `projects[workspace].mcpServers` 中当前工作区的 local server，结构依据
  [Claude Code 官方 scope 文档](https://code.claude.com/docs/en/mcp#scope-hierarchy-and-precedence)；
- `~/.codex/config.toml` 和 `.codex/config.toml` 中
  `[mcp_servers.<name>]` 下的 Codex 用户级与当前工作区 server，结构依据
  [Codex 官方 MCP 文档](https://developers.openai.com/codex/mcp/)；
- stdio 字段 `command`，可选字符串数组 `args`、字符串 `cwd`、字符串 map `env`。
- 跳过 Codex `enabled = false` server。`env_vars` 条目可以是名称，或
  `{ name, source = "local" | "remote" }` table；工具会验证结构，但不会从进程
  环境查找或输出其中命名的值。
- JSON 和 JSONC 文件可以以 UTF-8 BOM 开头；MCP Doctor 会在解析前移除它，兼容
  常见的 Windows 编辑器输出。
- VS Code 的 `${input:name}` 引用表示由客户端提供的输入，不会被当作未解析的进程环境占位符，
  也不会被展开。
- 多个已检查条目中完全同名或仅大小写不同的 stdio server 会被报告，但不会套用某个
  客户端专有的优先级规则。

带 `url`、HTTP、SSE 或其他非 stdio `type` 的远程条目会报告不支持，不会发起连接。
plugin 提供的 Codex MCP server 因顶层用户配置中没有启动命令而被忽略。在有独立需求
和兼容性证据前，YAML、MCP catalog、协议握手和 server 执行均不在范围内。

## 安全与隐私

MCP Doctor 默认只读。它读取指定 JSON、JSONC、Codex TOML 和文件元数据，并读取进程
`PATH` 来检查裸命令可发现性。它不会启动进程、发起网络请求、从进程环境中读取占位符
或 Codex `env_vars` 对应的值，也不会在报告中包含配置的环境变量值。占位符诊断使用
通用消息，不回显占位符 token；VS Code 的 `${input:name}` 引用因其值由客户端提供而
豁免，不会读取进程环境。环境变量 key 和 server 名称可能作为位置出现，但 human 输出
会转义终端控制字符。读取 `~/.claude.json` 时会忽略当前工作区之外的 project 条目。
分享配置前请先移除 secret。

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

测试覆盖支持的配置封装、JSONC 注释和尾逗号、当前进程 PATH 与 `PATHEXT`、确定和客户端
相关的路径诊断、占位符脱敏、VS Code 输入引用、终端安全输出、不支持的传输、Codex
TOML 解析与发现、坏输入、Claude Code user/local scope 选择、JSON 路径编码、CLI
退出码和“不执行命令”边界。

请阅读[产品规格](docs/PRODUCT_SPEC.zh-CN.md)了解证据门槛、发现路径和停止条件；修改前请阅读
[CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)、
[SECURITY.zh-CN.md](SECURITY.zh-CN.md) 和
[CHANGELOG.md](CHANGELOG.md)。

## 许可证

[MIT](LICENSE) Copyright Tinkora contributors.
