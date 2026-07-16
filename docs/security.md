# 安全与发布

## 自动检查

- CodeQL 扫描 Rust、JavaScript/TypeScript 和 GitHub Actions workflow。
- PSScriptAnalyzer 检查真实 PowerShell 文件；额外的 AST 检查阻止用户启动脚本使用动态执行、外部下载执行、修改 Defender、计划任务、执行策略和嵌套 PowerShell 等高风险能力。
- `cargo audit` 与 `npm audit --audit-level=high` 检查已知依赖漏洞。
- Dependabot 每周检查 Cargo、npm 和 GitHub Actions 依赖。
- 已跟踪密钥检查拒绝本地 Provider env 文件、私钥文件和常见真实 Key 形态。

这些检查不会使用或提交恶意脚本样例，也不会执行待检查脚本中的高风险代码。自动检查无法证明仓库绝对安全；任何新增启动或安装能力仍需要代码审查。

### RustSec 例外

`.cargo/audit.toml` 忽略 `RUSTSEC-2023-0071`，因为 SQLx 的锁文件会包含未启用的 MySQL 后端及其 `rsa 0.9.10` 依赖，而 NarraState 仅启用 SQLite。Security workflow 会在运行 `cargo audit` 前检查活跃依赖图；如果该版本的 `rsa` 实际被启用，检查会立即失败。该例外不得在移除活跃依赖图检查的情况下保留。

## GitHub 仓库设置

仓库管理员应在 GitHub 的 **Settings → Security → Advanced Security** 中确认启用：

- Dependabot alerts；
- Dependabot security updates；
- Secret Protection（当前界面中的总开关，包含 secret scanning）；
- Secret Protection 下的 Push protection；
- Private vulnerability reporting。

公开仓库的 secret scanning 可免费自动运行，因此界面中可能不会再出现一个独立的 “Secret scanning” 开关。私有仓库是否可启用完整 Secret Protection 取决于仓库所有者与 GitHub 方案。

建议同时保护 `main`，要求 CI、Security 和 Coverage 检查通过后再合并，并在发布功能可用时启用 Immutable Releases。

## Codecov

在 Codecov 安装 GitHub App 时选择 **Only select repositories**，只授权需要覆盖率的项目。NarraState workflow 使用 GitHub OIDC，不需要 `CODECOV_TOKEN`。Rust 与 Web 报告分别以 `rust`、`web` flag 上传并合并展示。

## SLSA 来源证明

推送 `v*` tag 会构建以下发行包：

- Linux x64；
- Windows x64；
- macOS ARM64；
- macOS x64。

GitHub `actions/attest` 默认生成 SLSA provenance，并通过短期 OIDC/Sigstore 身份签名。用户可以验证下载文件：

```bash
gh attestation verify <artifact> --repo gordonlu/NarraState
```

还应核对同名 `.sha256` 文件。来源证明验证“哪个 commit 通过哪个 workflow 生成了该文件”，不判断源码本身是否安全。

## API Key

不要在 Issue、聊天、命令参数、源码或受版本控制的文件中提供真实 API Key。文本与图片 Provider 的 Key 在网页设置中分别输入，默认保存在本机 `data/provider.env` 与 `data/image-provider.env`，不进入浏览器响应、SQLite、案件包、日志或生成报告。
