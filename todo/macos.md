# macOS follow-ups

## 1. No macOS CI job

`.github/workflows/ci.yml` only runs `ubuntu-latest`. Compile errors in
`platform/macos.rs` have gone unnoticed before because nothing builds the
crate for macOS in CI. Add a `macos-latest` runner to at least the
`test`/`clippy`/`fmt` jobs (or a dedicated
`cargo check --target x86_64-apple-darwin` job if full macOS runners are
too expensive to run on every push) so a regression gets caught
automatically next time.
