# loom-rust

Loom's one-shot client of Herdr's `wait.*` JSON API, plus a caller-local command
runner. Bash loom remains the public CLI and resolves node selectors to Herdr
agent ids. Herdr owns monitoring, deadlines, cycle checks and bounded escalation;
the client does not poll or retry mutations.

```sh
loom wait --on node:<node> --until done --deadline 2h --then parent
loom watch <node> --deadline 2h --then self
loom wait --on pr:<owner>/<repo>#N --until merged --deadline 4h --then self
loom wait --on mr:<group>/<project>!N --until merged --deadline 4h --then parent
loom wait --on ci:<owner>/<repo>#N --until passed --deadline 1h --then user
loom wait --on issues:<owner>/<repo>:.issues/<effort> --until closed --deadline 1d --then self
loom wait list --all
loom wait extend <wait-id> --deadline 30m
loom wait cancel <wait-id>
```

Declarations require both a positive duration (`ms`, `s`, `m`, `h`, `d`) and a
`self`, `parent` or `user` decider. Node `done` maps to Herdr's `complete` task
status; `complete`, `failed`, `closed`, `needs_parent` and `needs_user` are also
accepted. Reader sources require an explicit `--until` and preserve that state
literally. MR/CI GitLab shorthand becomes `mr:gl:<project>!N` or
`ci:gl:<project>!N`; `jira:KEY` is an alias for `issues:jira:KEY`, if the lane's
reader supports it. The plugin manifest declares `mr`, `pr`, `ci` and `issues`
readers through `cockpit read` at work or `homebase read` in personal. A missing
reader is a server refusal, never a silent local polling fallback.

A reader wait is declared only once the server has read its source. The server
reads it at once, and `loom wait` waits up to 40 s for a read to land. A server
that advertises `wait_read_retries` reads a failed first read again a few seconds
later, and `loom wait` waits those re-reads out, so one blip does not refuse the
wait (loom 113). Failed reads past the re-reads, a reader refusal, or no read at
all cancel the wait, and `loom wait` exits 1 saying what failed: a timeout, a
reader that gave no reason, or the reader's error. The server runs the reader
with its own environment, not the declaring pane's, so a ref that `loom check`
reads from a pane can still be unreadable to herdr (loom 110: cockpit's glab had
no `GITLAB_TOKEN` and read every private MR as a 404); the refusal says so for an
auth-shaped error. A reader that names its kind's `states` also gets an `--until`
outside them refused, with a hint when the state belongs to a related kind
(`passed` on a `pr:` is a `ci:` state). Once the first read has landed, the
declare prints the state it found, and `wait list` shows each wait's last read
or its run of failed reads.

`--summary` overrides the generated description. `--json` emits the API result
inside a `result` envelope. Lists include only the caller's waits, live by
default; `--all` selects all states, not all agents. Every invocation probes
`ping.capabilities.agent_waits`; built-in sources require their kind in
`wait_sources`, and plugin kinds require `wait_readers` or explicit source
advertisement. Herdr validates that a reader actually exists.

## Caller-local runs

```sh
loom run --name tests -- just test
loom wait --on run:tests --until success --deadline 2h --then parent
# Or start and declare together; deadline and decider are paired, never defaulted.
loom run --name checks --deadline 2h --then self --until success -- just check
```

`run` starts argv directly, not an implicitly evaluated shell string. To use
shell syntax, explicitly pass `sh -c '...'`. The detached supervisor remains in
the caller's process tree and inherits its environment, cwd and sandbox. stdin
is `/dev/null`; combined stdout/stderr is retained in `output.log`. The supervisor
stays alive for the command, then atomically renames a decimal exit code into
`exit`. Herdr receives `<pid>:<absolute exit file>` and never starts or signals
the command. `exit` (the default) accepts any exit; `success` requires code zero.

Records live under `$LOOM_RUN_DIR/<caller>/<name>`, or
`$XDG_STATE_HOME/loom/runs/<caller>/<name>` (default `~/.local/state/loom/runs`).
Names are 1–64 ASCII letters, digits, `-` or `_`, beginning with a letter or digit.
They are caller-scoped and never reused, preventing an older wait from seeing a
new command's exit. Keep the record/exit file while any wait refers to it. These
runs are not reboot-persistent; the server reports a vanished run without an exit
file as failed. A start-only run has no declared wait until `loom wait` is called.

On **host pi**, invoke run through the gate's **Bash path**, where the command is
approved and the process tree inherits the seatbelt. The native loom tool must
not execute a run outside that path; host-pi gate routing is a separate change.
The pi-sandbox shim and both script bridge modes refuse run: container PIDs are
not host-visible (Docker on macOS runs in a VM). The sandbox-run follow-up needs
a host-side `docker exec` client supervised by its host PID while the command
itself stays inside the container. It must not execute arbitrary commands on the
host as a substitute.

A lost declaration response may mean the mutation succeeded: inspect
`loom wait list --all` before retrying. If a one-step run starts but declaring
fails, its error identifies the already-running job and log; do not restart the
command. Declare the existing named run after checking the list.

## Installation and compatibility

The binary requires `LOOM_NODE` (the caller's resolved Herdr agent id) and uses
`HERDR_SOCKET_PATH`, or derives the socket from `HERDR_CONFIG_PATH`,
`XDG_CONFIG_HOME`/`HOME`, and `HERDR_SESSION`. It never starts a server. Unix
release assets ship beside Herdr as `loom-rust-<os>-<arch>`; install the matching
asset as `loom-rust` on PATH. `scripts/dogfood.sh` installs it with the other fork
binaries. Windows currently builds a stub, like the sidebar tool.

Bash falls back to the old `watch` only when the Rust binary is absent; it does
not emulate `wait` or `run` without a binary. Existing spawn-time watches retain
their legacy path. `watch --for` is available only on the absent-binary legacy
path; installed `watch` must declare a wait from the waiting node.

## Development

```sh
cargo test -p loom-rust --offline
cargo clippy -p loom-rust --all-targets --offline -- -D warnings
just check
```

Tests cover required policy arguments, duration overflow, capability probes,
source normalization, literal reader states, caller/name isolation, detached
output and atomic exits, direct argv preservation, and request fields against
Herdr's published schema. Socket tests use isolated Unix socket fixtures; they
require local bind/connect permissions, not network access.
