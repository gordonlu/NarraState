谜局AI（NarraState）发行包
===========================

Windows：双击 start.ps1，或在 PowerShell 中运行 .\start.ps1
Linux / macOS：在终端运行 ./start.sh

启动后打开 http://127.0.0.1:3000，在“设置”中填写 OpenAI-compatible
Base URL、模型和 API Key。文本与图片 Provider 可以分别配置。

API Key 默认保存在此目录的 data/provider.env，不会写入浏览器、SQLite、
案件包或日志。未配置 Provider 时仍可使用 Mock 模式。

验证发行包来源：
  gh attestation verify <压缩包路径> --repo gordonlu/NarraState

也可以对照同名 .sha256 文件检查下载完整性。
