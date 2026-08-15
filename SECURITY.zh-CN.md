# 安全与隐私

[English](SECURITY.md) | [简体中文](SECURITY.zh-CN.md)

MCP Doctor 读取指定 JSON、JSONC、Codex TOML 文件和文件系统元数据。它读取进程
`PATH` 来查找裸命令，并仅读取 home 目录位置来发现约定文件。虽然所选配置会在内存
中解析，但配置的环境变量值只用于静态空值和占位符检查，不会被输出、插值或用作命令
查找环境。工具也不会从进程环境读取 Codex `env_vars` 指定的值。占位符消息不会回显
识别到的 token。

human 输出会转义路径、server 名称、环境变量 key 和诊断中的终端控制字符，同时保留
普通 Unicode 文本。JSON 输出对非 UTF-8 文件系统路径使用有损表示。工具不会执行配置
中的命令或发起网络请求。

PATH 诊断只描述 MCP Doctor 当前进程，不证明 GUI 客户端继承了相同环境。除非绝对
`cwd` 提供确定基准，否则不会解析相对命令；相对工作目录始终只报告 warning。

不要在公开 Issue 中粘贴 secret。仓库启用 GitHub Private Vulnerability Reporting 后，
请通过该入口报告疑似漏洞，并提供不含凭据的最小复现。
