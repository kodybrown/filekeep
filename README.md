# filekeep

Run `filekeep` to keep versioned copies of individual files, the hosts file, and
environment settings. Each entry chooses its own destination, filename template,
and unchanged-content policy. There is no background service or built-in schedule.

## Build and run

Requires Rust 1.95 or newer. From PowerShell:

```powershell
cd "$HOME\Projects\filekeep"
cargo build --release --locked
cargo run --release --locked -- --dry-run
cargo run --release --locked
cargo run --release --locked -- --config-file 'C:\path\other-settings.toml'
```

Cargo outputs are configured in `.cargo/config.toml`, normally under
`C:/tmp/_rust/filekeep/target` on Windows or `/tmp/_rust/filekeep/target` on Unix.
Use Cargo's evaluated `target_directory`; the executable is under its `release`
directory as `filekeep.exe` on Windows (`filekeep` elsewhere).
The included `filekeep.yaml` carries forward both older apps' destinations and
date-based filenames. Preview it with `--dry-run` before a normal run. On Unix,
change the environment entry's scopes to `[process]`.

## Self-extracting installer

```powershell
cd "$HOME\Projects\doinstall"
.\build.cmd build
cd "$HOME\Projects\filekeep"
dotask create-filekeep-installer
```

The first command prepares matching doinstall tools after source updates; the
filekeep task only consumes prebuilt tools and never installs or builds doinstall
implicitly. It verifies/builds filekeep and produces one self-extracting setup
executable under `artifacts/installers/`, reporting its actual path. Runtime
installation needs neither dotask nor Cargo. Build and install are separate steps.

The installer includes the default YAML with hosts and machine/user environment
entries. Its interactive form lets the user select a backup folder and filename
template separately for each entry, plus ordinary application/PATH choices.
Both entries default to `skip_if_unchanged: true`. Filename templates remain
literal until filekeep runs. Installation does not run backups.

On Windows, settings are created at `%APPDATA%\filekeep\settings.yaml` (Roaming
AppData). Linux and macOS use `$XDG_CONFIG_HOME/filekeep/settings.yaml`, defaulting
to `~/.config/filekeep/settings.yaml`. Normal configuration discovery finds these
platform-specific locations.
The bundled example lives under `defaults/`, away from the executable so it cannot
shadow mutable user settings. Existing settings are preserved byte-for-byte on
install/upgrade; changing installer field values does not overwrite them. Edit the
existing YAML to change an established setup. Uninstall keeps settings by default;
actual backups are never installer-owned.

The task derives the version from Cargo. Use `--app-version` or `--build-stamp`
for an explicit package version/stamp. `.dotasks.yaml` selects the sibling doinstall
source checkout only to query Cargo's output for prebuilt native tools. Supply
`--doinstall-tools C:/path/to/tools` to use another matching tool set. Removing
`settings.doinstall.source` uses the installed builder's tool-path discovery.
Task adapters are tracked in `.dotasks-lock.yaml` for normal dotask synchronization.

To repeat Windows package acceptance without changing your active installation:

```powershell
dotask verify-installer --installer 'C:\path\reported-filekeep-setup.exe'
```

This uses temporary application/configuration/backup directories, checks persistent
PATH before and after, and exercises extraction, validation, custom outputs,
installed command discovery, duplicate skips, lock cleanup, reinstall, a distinct
upgrade package identity, and uninstall with retained settings/backups.

Unattended setup accepts the same values through doinstall's `--set` inputs:
`hosts-output-directory`, `hosts-filename`, `environment-output-directory`, and
`environment-filename`. To forward installer arguments through the self-extracting
launcher, place them after `--`. Pass `--set add-to-path=false` during isolated
acceptance, and use temporary application/configuration/command directories.

## Settings

YAML (`.yaml` or `.yml`), JSON, XML, and TOML are built in. They share one schema;
equivalent examples are in `filekeep.yaml` and `examples/`. No settings are
generated, rewritten, or migrated automatically.

```yaml
entries:
  - id: editor-settings
    source:
      type: file
      path: '${APPDATA}/Code/User/settings.json'
    destination: '${HOME}/Backups/editor'
    filename: '${HOST}-settings-${date:yyyyMMdd}.json'
    skip_if_unchanged: true
```

| Field | Meaning |
| --- | --- |
| `id` | Required, nonblank, unique entry identity. Keep it stable across runs. |
| `source.type` | `file`, `hosts_file`, or `environment`. |
| `source.path` | Required only for `file`; one file, without globbing or directory recursion. |
| `source.scopes` | Only for `environment`: `machine`, `user`, and/or `process`. Defaults to `[machine, user]`. |
| `destination` | Backup directory; created when a backup is needed. |
| `filename` | A basename template, without directory separators. |
| `skip_if_unchanged` | Defaults to `true`. Set to `false` to retain every successful capture. |

`hosts_file` uses `%SystemRoot%/System32/drivers/etc/hosts` on Windows and
`/etc/hosts` elsewhere. Use `type: file` to supply a different location.

Relative source and destination paths resolve against the selected settings
file's directory, including when invoked from another working directory. A
leading `~/` or `~\` expands to the user's home directory. Invalid output names
are reported instead of silently being sanitized into a different name.

Malformed settings, missing required fields, unknown fields/types, and an empty
entry list fail configuration loading before any backup starts. Entry validation
and runtime failures are reported separately; all other entries still run. Every
entry is reported as created, skipped, planned, or failed, followed by a summary.
Exit status is `0` when all entries succeed or skip, and `1` if any entry fails.
CLI argument errors use status `2`.

### Configuration discovery

`--config-file <path>` uses exactly that file (relative to the working directory)
and never falls back if it is missing or invalid. Without it, search in order:

1. Working directory: `<executable-stem>.yaml`, `.yml`, `.json`, `.xml`, `.toml`.
2. Executable directory: the same names.
3. Windows: `%APPDATA%\filekeep\settings.yaml`; when `APPDATA` is unset, empty,
   or relative, use `%USERPROFILE%\AppData\Roaming\filekeep\settings.yaml`.
   Linux/macOS: `$XDG_CONFIG_HOME/filekeep/settings.yaml`; when unset, empty, or
   relative, use `$HOME/.config/filekeep/settings.yaml`.

Windows ignores `XDG_CONFIG_HOME`; Linux/macOS ignore `APPDATA`. The app-specific
fallback directory is always `filekeep`, even if the executable is renamed.
For example, `work-settings.exe` first searches for
`work-settings.yaml` and the other extensions.

Existing Windows settings in an XDG directory are not moved automatically. Move
the file into `%APPDATA%\filekeep\settings.yaml` or select it with `--config-file`.

If multiple supported formats exist in the first matching directory, select one
explicitly with `--config-file`. An invalid selected file is an error; the app
does not silently load a lower-priority file. Only `settings.yaml` is searched at
the platform-specific fallback; other formats there can be selected explicitly.

### Templates

Environment tokens support `${VAR}`, `${env:VAR}`, `$(VAR)`, and `%VAR%`.
`HOME`/`USERPROFILE` resolve to home; `HOST`/`HOSTNAME`/`COMPUTERNAME` resolve to
the machine name. These aliases are case-insensitive. Other environment variable
names follow the platform's case rules. Empty values are allowed; unknown tokens,
cycles, and expansions deeper than 32 levels are errors.

Date tokens support `${date:format}`, `$(date:format)`, and `%date:format%`.
Supported .NET-style components are `yyyy`, `yy`, `MM`, `dd`, `HH`, `mm`, `ss`,
and `fff` (milliseconds), with punctuation as separators. For example:
`${date:yyyyMMdd-HHmmss-fff}`. Unsupported date components are errors. This is a
documented subset of the older helpers' .NET date formatting, not strftime or the
entire .NET formatting language. Every entry uses the same local timestamp
captured at run start; templates are evaluated when preparing that run.

### Duplicate detection and filename collisions

Comparison uses exact bytes: BOMs, line endings, trailing whitespace, and binary
content all matter. With `skip_if_unchanged: true`, compare the captured source
with that entry's previous successful backup, even when today's template gives a
different filename. A match creates no new backup and does not rotate files.

When a new backup would reuse an existing filename:

```text
settings-001.json   oldest archived version
settings-002.json   next archived version
settings.json       newest version, always the configured output name
```

Existing numbered archives stay in place. The displaced base gets one greater
than the highest existing suffix; gaps are not reused. Numbers have at least
three digits and continue beyond `999`. The suffix goes before the final
extension (`settings.tar-001.gz`); extensionless names also work. Date-stamped
files keep their own numbering series. Nothing is automatically pruned.

Small state files live in `<destination>/.filekeep/`, keyed by configuration path,
entry ID, and machine. They identify the previous successful filename without
guessing from arbitrary filenames, timestamps, or directory-wide extension
matches. Changing a config path, ID, machine, or destination starts a new history
identity; existing backup content is preserved. Preexisting backups without state
are preserved through collision archiving rather than adopted as previous state.
If the recorded latest file has been removed, a new backup is created. Unknown or
corrupt state is reported and preserved.

File capture streams through a temporary file in the destination. The old base
is preserved under its numbered name before the new base is atomically published;
on supported filesystems a hard link avoids another copy, with an exclusive-copy
fallback elsewhere. This keeps the base present during publication and prevents
overwriting existing archives. A destination lock coordinates filekeep processes;
Windows uses an exclusive delete-on-close marker, and Unix locks the existing
`.filekeep` directory. No lock marker remains after the owning process exits,
including process termination. JSON state files intentionally remain.
A busy destination fails its entries so they can be retried later. Output paths
that alias this run's settings, another entry's output, or a configured file
source are rejected. Existing output symlinks and directories are rejected.

`--dry-run` reads sources and previous state to report skips, filenames, and
planned archives, without creating files, directories, or locks. It cannot verify
write permissions or reserve a filename against concurrent changes. File sources
preserve content bytes; timestamps, ACLs, alternate data streams, and application
transactions are outside this tool's file-copy contract. Avoid modifying a source
while it is being captured if a consistent application-level snapshot is required.

### Environment captures

On Windows, `machine` and `user` read their registry environment keys without
expanding stored `%VAR%` references. `process` captures the inherited process
environment on every platform. Machine/user scopes are Windows-specific and
report an entry error on other platforms; no misleading process fallback is used.

Output is deterministic UTF-8 with LF endings, separate scope headings, sorted
variable names, quoted/escaped values, and readable semicolon-separated lists for
names containing `path`. A scope read failure fails the entire entry instead of
publishing an incomplete dump. Environment values are never printed to the
console, but the backup itself contains their values.

## Modular structure

`src/config.rs`, `src/sources/`, and `src/backup/` own the application schema,
source providers, backup state, orchestration, and error reporting. `main.rs` owns
the CLI and console output.

`power_code/` is an app-independent local library crate for multi-format settings,
discovery, path handling, templates, exact comparison, and safe publication. It
has no dependency on filekeep, which keeps its files suitable for future
synchronization with PowerCode's Rust utilities. The workspace builds one CLI
executable; there is no plugin runtime.

## Verification

```powershell
cd "$HOME\Projects\filekeep"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
cargo run --release --locked -- --dry-run
```

Tests use temporary directories. They cover exact byte preservation, chronological
rotation, numbering gaps, entries/configs with separate state, date changes,
failure continuation, output/source conflicts, locks, corrupt/missing state,
format equivalence, settings discovery, dry-run, and CLI exit behavior.

See `docs/legacy-review.md` for the original settings/behavior review.
