# 安全与隐私

[English](SECURITY.md) | [简体中文](SECURITY.zh-CN.md)

MCP Doctor 读取指定 JSON 文件和文件系统元数据。它读取进程 `PATH` 来查找裸命令，
并仅读取 home 目录位置来发现约定文件。虽然所选 JSON 会在内存中解析，但配置的环境
变量值绝不会出现在输出中，也不会从进程环境读取占位符对应的值。工具不会执行配置
中的命令或发起网络请求。

不要在公开 Issue 中粘贴 secret。仓库启用 GitHub Private Vulnerability Reporting 后，
请通过该入口报告疑似漏洞，并提供不含凭据的最小复现。
