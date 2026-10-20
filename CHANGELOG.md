# Changelog

## Unreleased

### 2026-10-20 Use platform-specific settings directories

- Discover and install settings in `%APPDATA%\filekeep\settings.yaml` on Windows,
  falling back to `%USERPROFILE%\AppData\Roaming` for an unavailable, empty, or
  relative `APPDATA`. Linux/macOS retain absolute `XDG_CONFIG_HOME` or
  `$HOME/.config`. Explicit configuration and working/executable-directory
  precedence remain unchanged.
- Use doinstall's reusable `app-data` template and isolate Windows installer
  acceptance under temporary roaming AppData with a separate XDG location.
- Verification: formatting, strict Clippy, all 31 Windows tests, release build,
  and temporary-root installer acceptance pass, including settings discovery,
  reinstall/upgrade preservation, uninstall, and unchanged persistent PATH.
  Linux/macOS native execution was not repeated for this change.

### 2026-10-19 Add a configurable self-extracting installer and transient locks

- Add dotask build/create-filekeep-installer tasks and tracked doinstall adapters
  to produce one native self-extracting setup executable from verified payloads
  and separately supplied prebuilt doinstall tools. Query evaluated Cargo paths;
  keep generated build output external and package artifacts ignored.
- Bundle hosts/environment defaults and offer independent backup-folder and
  filename-template fields. Initialize discoverable XDG settings through
  doinstall's reusable settings-files feature, preserve application template
  tokens, and leave existing settings unchanged on reinstall/upgrade. Defaults
  stay under defaults/ instead of shadowing editable settings beside the exe.
- Replace persistent backup lock markers with Windows exclusive delete-on-close
  handles and Unix directory locks. Preserve concurrency protection while
  releasing locks on normal/error exits and abrupt process termination.
- Align JSON/XML/TOML examples with the current user-selected YAML destinations;
  document the external Cargo output policy, installer workflow, configuration
  ownership, and reproducible temporary-root package acceptance.
- Verification: 31 filekeep/PowerCode tests, formatting, strict Clippy, native
  static-runtime Windows release build, and actual dotask package creation pass.
  verify-installer exercises self-extraction, read-only validation, custom outputs,
  installed config discovery, byte-match skips, lock cleanup, reinstall, a distinct
  upgrade identity, and uninstall preserving settings/backups; persistent user and
  machine PATH are unchanged. Native Unix installer acceptance is unverified.

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
