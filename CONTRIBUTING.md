# Contributing

Thanks for helping! Bug reports, translations and pull requests are welcome.

## Setup

1. Install Rust (stable), Node 22+, pnpm 10 and the [Tauri prerequisites](https://tauri.app/start/prerequisites/).
2. `pnpm install`
3. `pnpm app` to run the app with hot reload.

## Before opening a PR

```bash
pnpm check   # typecheck, vitest, rustfmt --check, clippy -D warnings, cargo test
```

- Keep prompts in `src-tauri/src/actions.rs` (single source of truth) and add a test when changing them.
- UI strings go in `src/i18n/en.json` **and** `src/i18n/pt-BR.json`.
- Never log or persist user text outside the opt-in history.
- New Tauri permissions must be justified in the PR (see `src-tauri/capabilities/default.json`).
- Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, `chore:`…); they make the generated release notes readable.

## Adding a language

Copy `src/i18n/en.json` to `<lang>.json`, translate, register it in `src/i18n/index.ts` and in the language selector (`src/settings/tabs/GeneralTab.tsx`). Tray/notification strings live in `src-tauri/src/i18n.rs`.
