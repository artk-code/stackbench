# Stackbench Cutover Checklist

Use this when creating the fresh `stackbench` repo and preparing the first commit from this trimmed v2 tree.

## Include
- `crates/swb-core`
- `crates/swb-config`
- `crates/swb-queue-sqlite`
- `crates/swb-receiver`
- `crates/swb-state`
- `crates/swb-eval`
- `crates/swb-jj`
- `crates/swb-adapters`
- `crates/swb-launcher`
- `crates/swb-cli`
- `desktop/`
- `docs/stackbench-workbench-macos.png`
- root workspace files needed to build the Rust and desktop packages
- the v2 architecture and contract docs that still describe the current product truth

## Exclude
- legacy Phase 0 tmux and browser docs that only describe the old system
- archived plan refresh bundles
- old screenshots with TRACE branding
- any leftover local `web/` artifacts; the tracked browser UI is already removed from this repo

## Decide At Repo Creation
1. whether the binary should stay `swb` or get the full `stackbench` name
2. whether the new repo should keep only v2 docs or keep a small migration note for context

## First Commit Goal
The first Stackbench commit should feel like a coherent product baseline:
- README is product-first
- screenshot matches the current desktop brand
- desktop smoke tests pass
- Electron packaging works locally
- v2 runtime and desktop docs match the shipped behavior

## Immediate Follow-On
1. validate `pnpm --dir desktop make` on Debian or Ubuntu
2. bundle a production `swb` binary into packaged desktop builds
3. improve login remediation for external-terminal flows
4. add persona, profile, and `gstack` views after packaging is stable
