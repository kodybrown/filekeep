# Original application review

Reviewed the source and shipped YAML files in `BackupEnvars` and
`BackupHostsFile` on 2026-10-19. Their shared Expander, FilePathExtensions, and
console-header helpers are byte-identical between the projects. Neither source
folder is currently a Git checkout.

## Settings inventory

Each app exposes exactly two settings. There is no configurable source,
duplicate policy, retention policy, or command-line settings override.

| App | Setting | Original value |
| --- | --- | --- |
| BackupEnvars | `backup_dir` | `${HOME}/Documents/Settings/(files)/Environment Variables/` |
| BackupEnvars | `file_format` | `${HOST}-ENVARS-${date:yyyyMMdd}.txt` |
| BackupHostsFile | `backup_dir` | `${HOME}/Documents/Settings/(files)/HOSTS file/` |
| BackupHostsFile | `file_format` | `${HOST}-HOSTS-${date:yyyyMMdd}.txt` |

The source and published YAML values agree. The first filekeep YAML retained these
destinations and naming templates in independent `environment` and `hosts` entries.
The current sample uses the user-selected `Documents/Settings-Windows/Files/`
destinations; installer defaults follow that current sample.
The old flat configuration is not a supported filekeep schema; no migration or
compatibility loader is included.

## Runtime behavior and disposition

| Original behavior | filekeep behavior |
| --- | --- |
| Exe-name YAML, current directory before executable directory | Exe-stem settings in YAML/YML/JSON/XML/TOML, then app-specific `settings.yaml` under roaming AppData on Windows or XDG on Linux/macOS; explicit `--config-file` bypasses discovery. |
| Hosts app repeats current-directory checks instead of checking executable directory | Search both actual directories. |
| Home/host aliases, four environment syntaxes, three date syntaxes | Retained; explicit errors for unresolved/cyclic tokens. |
| Any .NET date format | Explicit common-token subset: yyyy, yy, MM, dd, HH, mm, ss, fff. |
| One DateTime captured at process start | One local timestamp shared by all entries. |
| Relative backup directory resolves from CWD | Relative source/destination resolves from selected config directory. |
| Missing destination is created | Retained for real runs; dry-run writes nothing. |
| Invalid filename characters replaced with underscores | Reject invalid/reserved basenames to avoid silently redirecting backups. |
| Trimmed text comparison; duplicate suppression always enabled | Exact bytes; independent `skip_if_unchanged`, default true. |
| Latest is lexically greatest filename among hostname/extension matches | Track successful filename per config path, ID, machine, and destination. |
| Same-day output may overwrite an earlier backup | Archive existing base with next suffix; newest remains at base name. |
| Read hosts as text and re-encode when writing | Stream file bytes unchanged. |
| Hosts paths hardcoded to C:/Windows and /etc/hosts | Use SystemRoot on Windows and /etc/hosts on Unix. |
| Machine/user sections, raw Windows registry values, name sorting | Retained; process scope also available explicitly. |
| Names containing path formatted as semicolon-separated lists | Retained, with deterministic LF and escaping for special characters. |
| Registry failure silently falls back; scope errors may yield partial success | Fail the affected entry without publishing a partial environment dump. |
| One task per process; failure exits | Process independent entries; report every outcome and return failure if any entry fails. |
| Directory errors during backup enumeration are swallowed | State, directory, read, and write errors are explicit. |

The older FilePathExtensions contains additional unused general path methods;
these were reviewed but not ported wholesale. The shared header helper is display
only and adds no settings. The filekeep PowerCode boundary contains the reusable
primitives this application actually needs.
