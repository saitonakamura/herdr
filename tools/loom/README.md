# loom-rust

The first Rust slice of loom: a one-shot client of Herdr's `wait.*` JSON API.
Bash loom remains the public CLI and resolves loom selectors to Herdr agent ids.
The Rust client does not read loom's registry or maintain its own timers.

```sh
loom wait --on node:<node> --until done --deadline 2h --then parent
loom watch <node> --deadline 2h --then self
loom wait list --all
loom wait extend <wait-id> --deadline 30m
loom wait cancel <wait-id>
```

Declarations require both a positive duration (`ms`, `s`, `m`, `h`, `d`) and a
`self`, `parent` or `user` decider. `done` maps to Herdr's `complete` task status;
`complete`, `failed`, `closed`, `needs_parent` and `needs_user` are also accepted.
`--summary` overrides the generated description. `--json` emits the API result
inside a `result` envelope. Lists include only the caller's waits, live by default;
`--all` selects all states, not all agents.

Every invocation probes `ping` capabilities. A server lacking `agent_waits` is
refused, and node declarations require `node` in its advertised `wait_sources`.
Other source kinds are refused with a pointer to loom ticket 100 even when the
server advertises them. Herdr owns cycle checks, deadlines, persistence, fresh
source observations, the two-extension limit, and escalation. The client never
retries a mutation: a lost response may mean the declaration succeeded, so check
`loom wait list --all` before retrying.

The binary requires `LOOM_NODE` (the caller's resolved Herdr agent id) and uses
`HERDR_SOCKET_PATH`, or derives the socket from `HERDR_CONFIG_PATH`,
`XDG_CONFIG_HOME`/`HOME`, and `HERDR_SESSION`. It never starts a server. Unix
release assets ship beside Herdr as `loom-rust-<os>-<arch>`; install the matching
asset as `loom-rust` on PATH. `scripts/dogfood.sh` installs it with the other fork
binaries. Windows currently builds a stub, like the sidebar tool.

Bash falls back to the old `watch` only when the Rust binary is absent; it does
not emulate `wait` without a binary. Existing spawn-time watches retain their legacy path during this first slice.
`watch --for` is available only on the absent-binary legacy path; installed
`watch` must declare a wait from the waiting node.

## Development

```sh
cargo test -p loom-rust --offline
cargo clippy -p loom-rust --all-targets --offline -- -D warnings
just check
```

CLI tests cover required policy arguments, duration overflow, capability probes,
caller scoping, and request fields against Herdr's published schema. Socket tests
run the compiled binary against an isolated Unix socket fixture. They require
local Unix-domain socket bind/connect permissions, not network access.
