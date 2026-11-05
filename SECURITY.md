# Security policy

## Supported versions

Only the latest release receives security fixes. Installed apps update themselves from signed GitHub releases.

## Reporting a vulnerability

Please **do not open a public issue**. Use GitHub's private vulnerability reporting:
[Report a vulnerability](https://github.com/klysman08/Kaku/security/advisories/new).

Include affected version, OS, reproduction steps and impact. You can expect an acknowledgement within 7 days.

## Security model (summary)

- API keys live in the OS credential vault and are only read by the Rust backend; the webview never receives them.
- All provider requests are made from Rust over TLS using the OS certificate store.
- The webview runs with a restrictive CSP and a minimal set of Tauri capabilities (`src-tauri/capabilities/default.json`).
- Updates are verified with a minisign signature before installation.
- Release artifacts carry GitHub build-provenance attestations and SHA-256 checksums.
- Prompts wrap the user's text in `<input_text>` tags with an explicit instruction not to follow instructions found inside it (prompt-injection mitigation; not a guarantee).
