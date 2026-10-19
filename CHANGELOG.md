# Changelog

## 0.1.0 - 2026-10-19

- Combine BackupHostsFile and BackupEnvars in the Rust `filekeep` CLI, with
  independent file, hosts-file, and environment entries. Preserve both original
  destination/naming settings in the initial YAML and document the legacy review.
- Support YAML/YML, JSON, XML, and TOML; add `--config-file`, executable-name
  discovery in the working/executable directories, and app-specific XDG fallback.
- Compare exact bytes and track each entry's previous successful backup across
  filename/date changes. Keep the newest at its configured name and archive
  displaced copies using increasing suffixes, beginning with `-001` before the
  extension, without renumbering existing history.
- Process independent entries after errors, report per-entry outcomes and a
  summary, and return failure when any entry fails. Add a read-only `--dry-run`.
- Stage captures, preserve previous content before publication, lock destinations,
  and reject conflicting source/output/settings paths. Preserve corrupt or
  unknown state and report its affected entry instead of rewriting it.
- Keep app-independent configuration, paths, templates, and file I/O utilities in
  a local `power_code` library crate for future PowerCode synchronization.
- Verify with formatting, strict Clippy, 28 automated tests, an optimized Windows
  build, and read-only runs of all four example formats against the actual hosts
  and machine/user environment sources. Real backup destinations were not written
  during verification; write/rotation tests use temporary fixtures.
