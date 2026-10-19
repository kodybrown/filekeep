# filekeep agent instructions

- The main integration checkout is `~/Projects/filekeep`, directly on `develop`.
  Do not introduce a `local` directory, worktree, or feature branch unless asked.
- Preserve byte-exact file content. The newest backup keeps the configured name;
  displaced versions get increasing suffixes before the final extension. Never
  overwrite or renumber existing numbered backups.
- Process independent entries after failures and return a failing exit status if
  any entry fails. Keep environment values out of console output.
- Generic utilities belong in the local `power_code` crate. Keep backup-specific
  schema, source providers, and orchestration in the application crate.
- Do not run normal backups against the user's configured destinations merely to
  verify a change. Use temporary fixtures and `--dry-run` for real configuration.
- Update `CHANGELOG.md` for completed tasks and preserve useful code comments.
- Before committing scoped work, run `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo test --workspace --locked`, and `cargo build --release --locked`.
