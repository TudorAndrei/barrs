# `barrs doctor`: diagnostic command design

## Decision and boundaries

Add a future read-only `barrs doctor [--config PATH] [--socket PATH] [--json]
[--verbose]` command. It aggregates safe local checks and daemon-assisted
checks into one stable report. It does not start, stop, reload, mutate, or
reclaim a daemon; it does not invoke `launchctl`, Homebrew, or Rift actions by
default; it never reads or prints Lua source, full item snapshots, environment
values, service logs, or full user paths.

This document specifies a future public automation interface. It does not
implement a command, IPC request, or service probe.

## Probe inventory

| Check ID | Source today | Proposed behavior |
|---|---|---|
| `binary.version` | Cargo package version and `barrs --version` | Report normalized version. |
| `config.present` | `app::resolve_config_path` / config path selection | Check that the requested/default config path exists, without opening its contents in output. |
| `config.valid` | `config::load_config` and `validate-config` | Parse and validate locally; report only a normalized validation category. |
| `socket.present` | `ipc::default_socket_path` | Check whether a socket entry exists; do not unlink it. |
| `daemon.ping` | `Request::Ping`, `send_request` | Bounded ping; distinguish unavailable from malformed/error response. |
| `daemon.status` | `Request::Status` / `Response::Status` | When reachable, report running/item count/backend; omit config path. |
| `rift.backend` | `rift::select_backend().kind()` and `Request::RiftBackend` | Report selected local backend or daemon backend, with unavailable as a warning. |
| `display.summary` | `AppKitHost`/`BARRS_DEBUG_DISPLAYS` data path | New, redacted aggregate only: target mode, count, and whether a native target is available. Never expose AppKit objects or raw frames by default. |
| `service.hint` | New optional local hint | Detect only known launchd/Homebrew installation markers without executing a service command; absence is informational. |

The first implementation must label `display.summary` and `service.hint` as
new probes. They need separate platform abstractions; they must not reuse the
debug environment variable as a machine interface.

## Result contract

Every check has a stable ID and one of `pass`, `warn`, `fail`, or `skip`.
`pass` means the requested capability is available; `warn` means a usable
fallback or optional integration is absent; `fail` means the requested local
configuration/binary condition is broken; `skip` means a dependent probe could
not run. A check never throws away already collected results.

Human output is one stable line per check followed by a summary:

```text
PASS binary.version: barrs 0.2.3
PASS config.present: configuration file exists
PASS config.valid: configuration is valid
PASS daemon.ping: daemon responded
WARN service.hint: no managed service installation detected
Summary: 4 passed, 1 warning, 0 failed
```

`--json` emits exactly one JSON object to stdout and diagnostic errors only to
stderr. Version 1 has this schema; additional detail keys may be added, never
renamed or removed within the version:

```json
{
  "schema_version": 1,
  "overall": "warn",
  "checks": [
    {
      "id": "daemon.ping",
      "status": "pass",
      "message": "daemon responded",
      "details": { "latency_ms": 4 }
    }
  ],
  "summary": { "pass": 4, "warn": 1, "fail": 0, "skip": 0 }
}
```

Check order is the table order above. `details` is optional, JSON-object only,
and contains allow-listed machine values; a missing `details` field is not an
error. `message` is concise, actionable, and not intended for parsing. Scripts
must use `schema_version`, check `id`, and `status`.

## Exit status

| Condition | Exit code |
|---|---:|
| No `fail` checks, including warnings/skips only | 0 |
| One or more `fail` checks | 1 |
| Invalid command-line usage or unsupported option | Clap's existing usage error code |
| Internal command failure that prevents producing a report | 1 |

This preserves Plan 003's rule: daemon-declared failures are not reported as
successful command execution. A daemon that is intentionally not running is a
`warn` for the default diagnostic report, not an IPC transport crash. A doctor
mode that explicitly requires a running daemon may be added later as a separate
flag and would make `daemon.ping` failure fatal.

## Privacy and redaction policy

Default output is safe to paste into an issue. The implementation must enforce
these rules before human formatting and JSON serialization:

- Replace the current home directory prefix with `~`; do not print a config or
  socket path in normal messages/details at all.
- Never include Lua source, item labels/data, `dump-state`, handler names,
  environment values, command output, service logs, process arguments, or raw
  error backtraces.
- Normalize validation and IPC failures to stable categories such as
  `config_invalid`, `daemon_unavailable`, and `daemon_rejected`; retain only a
  bounded safe explanation written by barrs.
- Do not include display serial/vendor/model identifiers, raw display IDs,
  frame coordinates, or screen names by default.
- Bound all strings and arrays. An error received over IPC is untrusted for
  diagnostic rendering and must be categorized rather than copied verbatim.

`--verbose` is an explicit local opt-in, not a bypass. It may reveal normalized
config/socket paths and bounded display geometry useful for support, but still
must omit config contents, environment values, item state, logs, command output,
and hardware serial numbers. No `--include-secrets` option will exist.

## Foreground and service behavior

Doctor works whether the foreground daemon, a Homebrew/launchd service, or no
daemon is present. Socket ping/status are the authority for a running daemon;
the optional service hint never claims a service is healthy. In foreground mode
it reports the same checks. In service mode it may say that a known installation
marker was found, but tells the user to inspect their service manager separately
when the socket is unavailable. It must not require `launchctl` permissions.

## Implementation phases and tests

1. **CLI and pure report model** — add `DoctorArgs`, status types, deterministic
   sorting, human formatter, and JSON serializer in `src/cli.rs`/`src/app.rs`
   or a new diagnostic module. Tests: Clap parsing, golden human/JSON reports,
   exit codes, empty/skipped summaries, and schema field types.
2. **Safe local probes** — add config/socket/version probes around
   `src/app.rs`, `src/config.rs`, and `src/ipc.rs`. Tests: missing config,
   invalid config, socket absent, regular file at socket path, and path
   normalization without reading config content into output.
3. **Daemon protocol adapter** — reuse `Ping`/`Status` first; add an IPC
   diagnostic request only if a missing aggregate cannot be obtained safely.
   Tests: responsive daemon, unavailable daemon, malformed/error response,
   timeout, and an assertion that `Response::Error` yields nonzero only when it
   represents a required failed check.
4. **Platform summaries** — add isolated macOS display/service-hint adapters.
   Tests: pure descriptors plus manual foreground/Homebrew/launchd matrix.
   STOP if stable output would require AppKit object access outside the main
   thread, private APIs, config source, or arbitrary logs.
5. **Privacy review and documentation** — add redaction fixtures containing
   home paths, token-like environment text, Lua snippets, item data, service
   logs, display serials, and IPC error strings. Golden output must prove none
   leak under normal or verbose modes.

## Go / no-go criteria

Ship only after golden human/JSON, exit-status, unavailable-daemon/Rift/service,
and redaction tests pass on every supported platform. Stop the implementation if
a default probe needs Lua source, arbitrary log access, internal AppKit objects,
or unstable unredacted data to be useful. JSON IDs and meanings become public
API at the first release and require versioning for incompatible changes.
