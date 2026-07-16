# Security Policy

## Reporting a vulnerability

Please do not open a public issue for a suspected vulnerability or leaked credential.
Use GitHub's **Security → Advisories → Report a vulnerability** flow so the report can
be reviewed privately.

Include the affected version or commit, reproduction conditions, expected impact, and
any suggested mitigation. Do not include real API keys, personal data, or secrets in
the report.

## Scope

Security checks cover Rust and TypeScript source, GitHub Actions workflows,
PowerShell startup scripts, tracked secret patterns, and known dependency
vulnerabilities. Passing checks reduce risk but do not certify that the software is
free of every vulnerability.
