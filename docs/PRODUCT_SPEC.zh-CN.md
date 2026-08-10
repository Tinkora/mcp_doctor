# MCP Doctor 产品规格

[English](PRODUCT_SPEC.md) | [简体中文](PRODUCT_SPEC.zh-CN.md)

## 有证据的问题

本地 MCP 配置经常在协议请求开始前就启动失败：客户端进程的 `PATH` 中没有
`npx`，Node 来自宿主应用没有继承的 NVM shell，或者 Windows 路径和工作目录
错误。MCP 官方 servers 的 [#40](https://github.com/modelcontextprotocol/servers/issues/40)、
[#64](https://github.com/modelcontextprotocol/servers/issues/64)、
[#447](https://github.com/modelcontextprotocol/servers/issues/447)，Cline 的
[#1948](https://github.com/cline/cline/issues/1948)、[#902](https://github.com/cline/cline/issues/902)，
Continue 的 [#4791](https://github.com/continuedev/continue/issues/4791)、
[#7509](https://github.com/continuedev/continue/issues/7509)，GitHub MCP Server 的
[#1396](https://github.com/github/github-mcp-server/issues/1396)，以及相关
Stack Overflow 问题都反复出现这些故障。

## 最小可用结果

给定 JSON 配置或少量约定的本地配置路径，`mcp-doctor` 静态报告每个本地
`stdio` server 是否具备启动前提。它检查命令可发现性、显式命令路径、工作目录
和未解析占位符，不执行命令，也不从进程环境读取占位符对应的值。人类输出适合终端，
JSON 输出可供 CI 包装使用，且不会包含配置的环境变量值。

## 支持边界

- 顶层为 `mcpServers` map 的 JSON（Claude Desktop、Cline 风格）。
- 顶层为 `servers` map 的 JSON（VS Code 风格，条目可带 `type: "stdio"`）。
- server 字段：`command`、`args`，可选 `cwd` 和字符串 `env`。
- 带 `url`、`http`、`sse` 或其他非 stdio `type` 的远程条目不检查，只报告明确的
  不支持传输诊断。
- YAML、TOML、catalog、协议握手和远程传输暂不支持，等待独立兼容性证据。

自动发现保持保守：当前工作区的 `.vscode/mcp.json`、`.mcp.json`、
`.cursor/mcp.json`，以及当前平台上 Claude Desktop、Cline、Cursor 的已知用户配置路径。

## 检查与安全

- 缺失或空的 `command` 是错误。
- 裸命令只根据 MCP Doctor 当前进程的 `PATH` 检查，且不打印 `PATH` 或其条目；结果
  不宣称复现 GUI 客户端环境，也不使用配置中的 `env.PATH`。Windows 查找使用当前
  `PATHEXT`，缺失时使用平台默认值。
- 绝对命令路径和绝对工作目录会检查存在性，Unix 上还检查可执行权限。只有绝对 `cwd`
  提供确定基准时才继续检查相对命令；其他相对命令和工作目录只报告客户端上下文
  warning，避免给出猜测性的错误。
- 未解析的 `${VAR}`、`$VAR`、`%VAR%`、`{{VAR}}` 占位符会被报告，但不会回显 token
  或配置值。
- 诊断位置可以出现环境变量 key。配置值只在内存中用于空值和占位符静态检查，不做
  插值、不用于命令查找，也不输出。
- 默认命令只读文件和元数据；本版本没有 `--run`，也不会隐式启动进程。
- human 输出会转义配置内容中的终端控制字符；非 UTF-8 JSON 路径使用有损表示，避免
  命令成功却没有产生 JSON。

## CLI 契约

```text
mcp-doctor [OPTIONS] [CONFIG ...]
  --format human|json     输出格式（默认 human）
  --ci                    检查错误时退出 1，输入错误退出 2
  --no-discover           只检查显式 CONFIG 路径
```

没有显式路径时执行发现。发现路径不存在不是错误；显式指定但不存在的路径是输入错误。

## 与 MCP Inspector 的区别

MCP Inspector 是协议级调试工具：可以启动 server、执行握手、检查 tools，并支持
stdio 和远程传输。MCP Doctor 是协议启动前的预检层，专门处理那些阻止 Inspector
启动的本地配置问题：发现客户端配置、静态检查进程前提、默认不启动 server，并提供
适合 CI 的 JSON/退出码。它不替代 Inspector，也不宣称协议兼容性或 server 正确性。

## 成功与停止条件

当用户无需暴露 secret 或运行 server，就能从配置中定位缺失的 `npx`/Node 路径、错误
工作目录或未解析占位符时，MVP 即成功。没有独立兼容性证据时停止扩展解析器；新增
客户端格式或进程执行模式前，先用具体 issue/discussion 反馈验证需求。
