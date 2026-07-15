# 确定性校验

案件发布前依次经过解析、规范化、编译、语义校验和自动模拟。LLM 不参与通过与否的判断。每个错误包含稳定 `code`、字段 `path`、说明和相关 ID，CLI 失败返回非零退出码。

校验覆盖引用完整性、时间线、证据可达性、结案要件、披露图无环与可达、变体差异和公平性。`DisclosureGraph` 是完整认罪的唯一入口；非责任角色出现主罪认罪节点会返回 `FALSE_SUSPECT_CAN_CONFESS`，不能通过压力值或矛盾次数绕过。

仓库提供四个静态无效 Golden Packages：差异不足、披露死路、隐藏关键证据和错误嫌疑人可认罪。其 `expected.json` 固定预期错误码，防止校验退化为始终成功。

```bash
cargo run -p narrastate-server -- case validate cases/golden-invalid/hidden-required-evidence
```
