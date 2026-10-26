# 045: live flip check, 2026-10-26

Evidence for [045](../045-flip-report-after-colours.md). Primary mac, kitty 0.49.2,
a release build of `fork/045-flip-report-after-colours` at `d0e50067` (sha256
prefix `1bb6bfcbdf32a26a`), Claude Code 2.1.310.

## Setup

- One throwaway kitty process from a clean env (`env -i` plus `HOME`, `PATH`,
  `CLAUDE_CONFIG_DIR=~/.claude-personal`), its own OS window, running the client
  of a throwaway session `flip045` on that binary. It was unfocused for every flip.
  Server stopped, kitty closed and session dir removed afterwards.
- `w1:p1`: [043's probe](./043-flip-chain-measurements.md#the-probe) with `--2031`,
  with one change. When a report lands inside a query round, the probe now logs
  whether it came before or after that round's `OSC 11` reply. An answer produced
  before the report reached the pane may be stale without being a fault:

  ```python
  p11 = next((m.start() for m in OSC.finditer(seg) if m.group(1) == b"11"), -1)
  matches = list(R997.finditer(seg))
  for m in matches[:-1]:  # the last ?997 before DA1 answers our ?996n
      where = "before-osc11" if p11 >= 0 and m.start() < p11 else "after-osc11"
      w("REPORT %s (inside round, %s)" % (m.group(1).decode(), where))
  ```

  None of the four reports below fell inside a round, so the change did not come
  into play.
- `w1:p2`, `w1:p3`: Claude (`theme: "auto"`), both started before the first flip,
  judged by 043's markers over the visible screen (dark fg `#999999`/`#ffc107`,
  light `#666666`/`#966c1e`).
- Flips: `osascript … set dark mode to true|false`, four of them 10 s apart, a
  census 6 s after each. Then `defaults write -g
  AppleInterfaceStyleSwitchesAutomatically -bool true`, which read back as `1`. The
  desktop ended light with `AppleInterfaceStyle` absent, the same as before.

## Results

| flip | report at probe | first `OSC 11` after it | stale window | `w1:p2` Claude | `w1:p3` Claude |
| --- | --- | --- | --- | --- | --- |
| → dark | .971 | dark, +4 ms | 0 | dark 14 / light 0 | dark 14 / light 0 |
| → light | .382 | light, +3 ms | 0 | dark 0 / light 14 | dark 0 / light 14 |
| → dark | .707 | dark, +14 ms | 0 | dark 14 / light 0 | dark 14 / light 0 |
| → light | .082 | light, +0 ms | 0 | dark 0 / light 14 | dark 0 / light 14 |

Before the fix, 043 measured stale windows of 74–98, 41–65 and 26–39 ms and ≤4 ms
(0 in two samples), and pre-flip Claude panes kept the wrong side on flip 2.

The second flip shows the new order arriving at the pane. kitty's new foreground
lands first, then the new background, and the report right behind it:

```text
1793005722.379 fg=rgb:3737/6060/bfbf bg=rgb:1a1a/1b1b/2626 996=1   <- OSC 10 reply applied, report held
1793005722.382 REPORT 2 (unsolicited)
1793005722.385 fg=rgb:3737/6060/bfbf bg=rgb:e1e1/e2e2/e7e7 996=2   <- light, first answer after the report
```

Unlike 043's Claude panes, neither pane kept a mixed screen after any flip: all
14 markers were on the new side every time.

## The analyser

Run with `python3 -I analyse.py probe.log`. For each report it prints the first
background after it and the last stale one; a stale window is non-zero only if
the probe saw the old background after the report.

```python
import re, sys

LINE = re.compile(r"^(\d+\.\d+) (.*)$")
BG = re.compile(r"bg=rgb:([0-9a-f]+)/([0-9a-f]+)/([0-9a-f]+)")
REPORT = re.compile(r"REPORT (\d)")


def side(m):
    r, g, b = (int(x[:2], 16) for x in m.groups())
    return "light" if 0.299 * r + 0.587 * g + 0.114 * b > 128 else "dark"


for path in sys.argv[1:]:
    events = [(float(m.group(1)), m.group(2))
              for m in (LINE.match(l.strip()) for l in open(path)) if m]
    for i, (t, text) in enumerate(events):
        rep = REPORT.search(text)
        if not rep:
            continue
        want = "dark" if rep.group(1) == "1" else "light"
        last_stale, first_new = None, None
        skip_same_round = "after-osc11" in text
        for t2, text2 in events[i + 1:]:
            if t2 > t + 2.0 or REPORT.search(text2):
                break
            bg = BG.search(text2)
            if not bg:
                continue
            if skip_same_round:
                skip_same_round = False
                continue
            if side(bg) != want:
                last_stale = t2
            elif first_new is None:
                first_new = t2
        print(rep.group(1), want, t, first_new, last_stale)
```
