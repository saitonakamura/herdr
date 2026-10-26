# 043: flip chain measurements, 2026-10-26

Raw evidence for [043](../043-light-dark-flip-chain.md). Primary mac, kitty 0.49.2,
herdr `0.9.5+fork.1` (`~/.local/opt/herdr-fork/bin/herdr`), Claude Code 2.1.310,
codex-cli 0.162.1, pi 1.1.0 (the `pi-sandbox:1.1.0` container the `pi` shim runs).

## Setup

- Two throwaway kitty processes, started from a clean env (`env -i` plus
  `CLAUDE_CONFIG_DIR=~/.claude-personal`, `CODEX_HOME=~/.codex-personal`), four OS
  windows, each running the herdr client for one throwaway session (`flip043a`..`d`)
  behind a pty relay. The relay logs every byte in both directions with wall-clock
  time, so kitty's report and its colour replies are timestamped where the client
  receives them. Servers stopped and session dirs removed afterwards.
- In every session a **probe pane**: sets mode 2031, then every 20 ms writes
  `OSC 10 ?`, `OSC 11 ?`, `CSI ?996n` and a DA1 fence, and logs whenever the
  answers change. On a report it fires a round at once, the way Claude does. In A,
  a second probe leaves mode 2031 off. Script below.
- Harness panes started before the flip: A held Claude (`theme: "auto"`), codex,
  pi as stored, pi `--use-theme light/dark`, and Claude forced to `dark` and to
  `light` with `--settings '{"theme":…}'`, which leaves the stored setting alone.
  After flip 1, fresh Claude, codex and pi `light/dark` panes went into A and into B.
- Side judged by counting SGR colours over the whole visible screen
  (`herdr pane read --source visible --format ansi`) against marker colours, with
  ground truth taken two ways. The first was a pty harness outside herdr, answering
  `OSC 10/11/4` with the exact Tokyo Night or Tokyo Night Day values. The second was
  the forced-theme Claude panes in herdr, whose counts matched the harness exactly.

| harness | dark markers | light markers |
| --- | --- | --- |
| Claude | fg `#999999`, `#ffc107` | fg `#666666`, `#966c1e` |
| codex | composer bg `#353640` | composer bg `#d8d8dd` |
| pi | fg `#5f87d7`, `#d78700`, `#afafaf` | fg `#afafd7`, `#875f00`, `#5f5f87` |

## The appearance on this mac

`AppleInterfaceStyleSwitchesAutomatically = 1` (Auto). AppKit's effective appearance
was `NSAppearanceNameDarkAqua` at 09:43 while `defaults read -g AppleInterfaceStyle`
reported the key absent, i.e. "light" to `herdr-theme-nudge`'s `mac_side()`. A fresh
kitty instance agreed with AppKit: it answered `OSC 11` dark and reported `?997;1`.

Flip 1: `osascript … set dark mode to false`, at 1793001051.3 (dark → light; this
turned Auto off). Flip 2: the user, in System Settings, at about 1793001354.3
(light → dark; it ended on explicit Dark).
No kitty window got a focus-in during either flip (the relay logs show no `CSI I`
after 1793000604). Every client measured was in an **unfocused** window.

## kitty → client (relay logs, seconds after the epoch, fractional part)

```text
flip 1   A: REPORT 2 .042  client writes theme query .066  OSC 10/11 .090  OSC 4 x253 until .151
         B: REPORT 2 .048  query .087  OSC 10/11 .095  OSC 4 until .171
flip 2   A: REPORT 1 .308  query .309  OSC 10/11 .334  OSC 4 until .393
         B: REPORT 1 .308  query .309  OSC 10/11 .334  OSC 4 until .393
         C: REPORT 1 .316  query .328  OSC 10/11 .349  OSC 4 until .386
         D: REPORT 1 .316  query .333  OSC 10/11 .347  OSC 4 until .390
```

## What the probe pane got

| sample | report at pane | last stale `OSC 11` | first new `OSC 11` | stale window after the report |
| --- | --- | --- | --- | --- |
| flip 1, A | .099 | none after the report | .104 | 0 (new colours landed first) |
| flip 1, B | .104 | .178 | .202 | 74–98 ms |
| flip 2, A | .342 | .383 | .407 | 41–65 ms |
| flip 2, B | .353 | .379 | .392 | 26–39 ms |
| flip 2, C | .378 | .378 | .382 | ≤4 ms |
| flip 2, D | .380 | none | .381 | 0 |

- `CSI ?996n` answered the new side in the same round as the report, every time.
  Only `OSC 10/11` lag.
- The probe with mode 2031 off received no report in either flip. Its `OSC 11`
  answers changed at the same moment as its sibling's.
- In three samples the new `OSC 11` reached the pane before kitty had finished the
  256 palette replies (flip 1 A, flip 2 B and C). So the window is one round trip
  for `OSC 10/11` plus server time. It is not the 258-reply wait that 007 and the
  nudge's header assumed. The client forwards each colour reply as it parses it
  (`src/client/shell_runtime.rs`, `ClientShellHostTheme::DefaultColor`).

Excerpt, flip 1, session B:

```text
1793001052.104 REPORT 2 (inside round)
1793001052.105 fg=rgb:c0c0/caca/f5f5 bg=rgb:1a1a/1b1b/2626 996=2   <- report says light, colours still dark
1793001052.178 fg=rgb:c0c0/caca/f5f5 bg=rgb:1a1a/1b1b/2626 996=2
1793001052.202 fg=rgb:3737/6060/bfbf bg=rgb:e1e1/e2e2/e7e7 996=2   <- light
```

## herdr's own UI and the sidebar

Replaying session A's client output into a screen model: tab bar `#1a1b26`/`#24283b`,
accent `#7aa2f7` (tokyo-night) at 1793001051.5, and `#e1e2e7`/`#c4c8da`/`#2e7de9`
(tokyo-night-day) by 1793001052.10, i.e. within 60 ms of the report. Flip 2 reversed
it by 1793001354.45 in A and B. The sidebar columns draw the terminal-default background, ANSI
palette indices, and the fixed hexes from config (`#757ead`, the brand colours). The
first two are resolved by kitty, so they flip with it. The fixed hexes stay as
configured, by design.

## Harness panes (census)

| pane | before flip 1 (dark) | after flip 1 (light) | after flip 2 (dark) |
| --- | --- | --- | --- |
| A Claude `auto` | dark | light, but the static welcome block keeps 22 `#999999` spans | still light, so **wrong** |
| A Claude forced dark / light | dark / light | dark / light | dark / light |
| A codex | dark | **dark (wrong)** | dark |
| A pi stored (`"light"`) | **light (wrong)** | light | **light (wrong)** |
| A pi `light/dark` | dark | light | **light (wrong)** |
| A Claude, started after flip 1 | — | light | **light (wrong)** |
| A codex, started after flip 1 | — | light | **light (wrong)** |
| A pi `light/dark`, started after flip 1 | — | light (after the forge below) | **light (wrong)** |
| B Claude, started after flip 1 | — | light | **light (wrong)** |
| B codex, started after flip 1 | — | light | **light (wrong)** |
| B pi `light/dark`, started after flip 1 | — | light | **light (wrong)** |

Session A lost no race on flip 1 and lost the race on flip 2. B was not measured
with harnesses on flip 1 and lost on flip 2. The probe numbers above predict
exactly this.

## Harnesses outside herdr (pty harness)

- **Claude 2.1.310**: sets `?2031h` at startup, queries `OSC 11` once at startup.
  On `?997` it re-queries `OSC 11` within 1 ms and themes from the answer, not
  the payload: a `?997;1n` (dark) report answered with light painted light, and a
  `?997;2n` report answered with dark stayed dark.
- **codex 0.162.1**: never sets 2031; queries `OSC 10/11` once at startup. A `?997`
  report, focus-out and focus-in all produced no query and no repaint.
- **pi 1.1.0**: with the stored `"theme": "light"` (in `~/.pi-work/settings.json`)
  it never sets 2031 and renders light on a dark answer. With `--use-theme
  light/dark` it sets 2031 once past the trust prompt. On `?997` it re-queries
  `OSC 10/11` plus the 16 `OSC 4` entries at once, and themes from the answer. A
  light report answered with dark stayed dark, the same trap as Claude.

## The nudge

Run by hand against session A (`HERDR_SOCKET_PATH` set, fork `herdr` first on `PATH`).
The Claude pane and the pi `light/dark` pane were first forced to the wrong side, with
one write each of `?997;1n` plus a forged dark `OSC 11`:

```text
forged      A Claude new   dark    A pi light/dark new  dark
herdr-theme-nudge light               -> nudged 4 claude pane(s) -> light
herdr-theme-nudge --pane <pi> light   -> nudged <pi> -> light
herdr-theme-nudge --pane <codex> light
after       A Claude new   light   A pi light/dark new  light   A codex  dark (unchanged)
            forced-theme Claude panes unchanged
```

The installed launchd nudge did not run for either flip:

- `~/Library/Logs/herdr-theme-nudge.log` stayed at 52 lines (mtime 07:01) through both
  flips. The `--watch` process (pid 1203, started Oct 14, helper pid 2009) was alive
  with no stuck child. Why its helper printed nothing was not established.
- Flip 1 would have been dropped anyway. The state file already said `light` (stale
  from Auto), and `mac_side()` said `light` after the flip, so `act_on_change` saw no
  change.
- The 5-minute backstop caught flip 2 at 09:57:51, about two minutes late, and logged
  `nudged 0 claude pane(s) -> dark` twice. All 54 lines of the log say `nudged 0`,
  because launchd's bare `herdr` reaches only the default session, and the user's
  agents run in the named sessions `fork`, `personal` and `work`.

## The probe

```python
"""In-pane sampler: what does the terminal answer, and when, around a flip.

  python3 probe.py <logfile> [--2031] [--interval-ms 20]
"""

import os, re, select, sys, termios, time, tty

log_path = sys.argv[1]
mode2031 = "--2031" in sys.argv
interval = 0.02
if "--interval-ms" in sys.argv:
    interval = int(sys.argv[sys.argv.index("--interval-ms") + 1]) / 1000

QUERY = b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\\x1b[?996n\x1b[c"
OSC = re.compile(rb"\x1b\](10|11);(rgb:[0-9a-fA-F/]+)(?:\x07|\x1b\\)")
R997 = re.compile(rb"\x1b\[\?997;(\d)n")
DA1 = re.compile(rb"\x1b\[\?[\d;]*c")
log = open(log_path, "a", buffering=1)


def w(line):
    log.write("%.3f %s\n" % (time.time(), line))


fd = sys.stdin.fileno()
old = termios.tcgetattr(fd)
tty.setraw(fd)
os.write(1, b"\x1b[?2031h" if mode2031 else b"\x1b[?2031l")
buf, last, verbose_until = b"", None, 0.0


def read_some(timeout):
    global buf
    r, _, _ = select.select([fd], [], [], timeout)
    if r:
        buf += os.read(fd, 4096)
        return True
    return False


def round_once(reason):
    # one query round; a ?997 beyond the 996 answer is an unsolicited report
    global buf, last, verbose_until
    t_send = time.time()
    os.write(1, QUERY)
    deadline = time.time() + 1.0
    while not DA1.search(buf) and time.time() < deadline:
        read_some(0.05)
    m = DA1.search(buf)
    seg, buf = (buf[: m.end()], buf[m.end():]) if m else (buf, b"")
    osc = dict((k.decode(), v.decode()) for k, v in OSC.findall(seg))
    reps = [x.decode() for x in R997.findall(seg)]
    for extra in reps[1:]:
        w("REPORT %s (inside round)" % extra)
        verbose_until = time.time() + 3
    cur = (osc.get("10", "-"), osc.get("11", "-"), reps[0] if reps else "-")
    if cur != last or time.time() < verbose_until:
        w("fg=%s bg=%s 996=%s rtt=%.1fms %s" % (cur + ((time.time() - t_send) * 1000, reason)))
    last = cur


try:
    next_round = time.time()
    while True:
        if time.time() >= next_round:
            round_once("tick")
            next_round = time.time() + interval
        if read_some(max(0.0, next_round - time.time())):
            if b"\x03" in buf:
                break
            if R997.search(buf) and not DA1.search(buf):
                for m in R997.finditer(buf):
                    w("REPORT %s (unsolicited)" % m.group(1).decode())
                buf = R997.sub(b"", buf)
                verbose_until = time.time() + 3
                round_once("after-report")
finally:
    os.write(1, b"\x1b[?2031l")
    termios.tcsetattr(fd, termios.TCSAFLUSH, old)
```
