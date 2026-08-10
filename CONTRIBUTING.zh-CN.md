# 贡献指南

[English](CONTRIBUTING.md) | [简体中文](CONTRIBUTING.zh-CN.md)

请从可复现的 MCP 客户端工作流和故障证据开始。提案需要说明客户端/配置格式、
最小用户结果、现有替代方案和停止条件。Pull Request 保持聚焦，并为每个行为变化
补充测试。

项目明确不执行进程，也不支持远程传输。不要仅凭想象添加配置格式、server 启动模式
或网络功能；在 PR 中附上独立兼容性或用户证据。

提交前运行：

```bash
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

用户可见范围变化时同步更新两份 README 和产品规格。提交使用英文 Conventional
Commits，不要提交 secret 或真实 MCP 环境变量值。
