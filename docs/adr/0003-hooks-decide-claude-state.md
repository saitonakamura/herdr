# Hooks decide a Claude pane's state; the screen is the fallback

Upstream decides a Claude pane's idle, working or blocked state only from what is
on screen, and overwrites any reported state on the next check. That misreads a
question asked in prose, a turn that ended while background work runs, a tall
pane's old spinner, and a stale title. The user's dotfiles patch each case with a
local manifest that must be re-merged on every upstream update. The fork makes
Claude a full lifecycle integration, like pi: Claude Code hooks report the state
and why a pane is blocked, and screen rules remain only for what hooks cannot
see, such as permission dialogs.

## Consequences

`herdr integration install claude` installs more hooks, in every Claude profile.
Upstream's Claude manifest updates matter less, and the dotfiles' override
shrinks to what the hooks cannot cover.
