# Invalid Golden Packages

这些案件包必须被 v0.2 正式加载器拒绝，分别固定以下错误：

- `insufficient-divergence`：伪真相变体；
- `disclosure-deadlock`：披露路径不可达；
- `hidden-required-evidence`：关键证据不可发现；
- `false-suspect-confession`：非责任角色拥有主罪认罪图。

重新生成：

```bash
cargo run -p narrastate-case --example generate_invalid_goldens -- cases/golden-invalid
```
