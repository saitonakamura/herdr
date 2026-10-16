#!/bin/sh
# installed by herdr
# managed by herdr; reinstalling or updating the integration overwrites this file.
# add custom hooks beside this file instead of editing it.
# HERDR_INTEGRATION_ID=claude
# HERDR_INTEGRATION_VERSION=12

set -eu

action="${1:-}"
hook_input_file="$(mktemp "${TMPDIR:-/tmp}/herdr-claude-hook.XXXXXX")" || exit 0
trap 'rm -f "$hook_input_file"' EXIT HUP INT TERM
cat >"$hook_input_file" 2>/dev/null || true

case "$action" in
  session|state) ;;
  *) exit 0 ;;
esac

[ "${HERDR_ENV:-}" = "1" ] || exit 0
[ -n "${HERDR_SOCKET_PATH:-}" ] || exit 0
[ -n "${HERDR_PANE_ID:-}" ] || exit 0
command -v python3 >/dev/null 2>&1 || exit 0

HERDR_ACTION="$action" HERDR_HOOK_INPUT_FILE="$hook_input_file" python3 - <<'PY'
import json
import os
import random
import socket
import time

source = "herdr:claude"
action = os.environ.get("HERDR_ACTION", "")
pane_id = os.environ.get("HERDR_PANE_ID")
socket_path = os.environ.get("HERDR_SOCKET_PATH")
hook_input_file = os.environ.get("HERDR_HOOK_INPUT_FILE")

QUESTION_SENTINEL = "⏳ awaiting your answer"
BACKGROUND_SENTINEL = "\U0001f504 awaiting background work"

if not pane_id or not socket_path:
    raise SystemExit(0)

hook_input = {}
if hook_input_file:
    try:
        with open(hook_input_file, encoding="utf-8") as handle:
            content = handle.read()
        if content.strip():
            hook_input = json.loads(content)
    except Exception:
        hook_input = {}

if "CURSOR_VERSION" in os.environ or "cursor_version" in hook_input:
    raise SystemExit(0)
hook_event_name = str(hook_input.get("hook_event_name") or "")
is_subagent = bool(hook_input.get("agent_id"))
session_id = hook_input.get("session_id")
agent_session_id = session_id if isinstance(session_id, str) and session_id else None
report_seq = time.time_ns()


def send(method, params):
    request = {
        "id": f"{source}:{int(time.time() * 1000)}:{random.randrange(1_000_000):06d}",
        "method": method,
        "params": params,
    }
    try:
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        client.settimeout(0.5)
        client.connect(socket_path)
        client.sendall((json.dumps(request) + "\n").encode())
        try:
            client.recv(4096)
        except Exception:
            pass
        client.close()
    except Exception:
        pass


def report_state(state, seq, blocked_reason=None, background=False):
    if not agent_session_id:
        return
    params = {
        "pane_id": pane_id,
        "source": source,
        "agent": "claude",
        "state": state,
        "seq": seq,
        "agent_session_id": agent_session_id,
    }
    if blocked_reason:
        params["blocked_reason"] = blocked_reason
    if background:
        params["background"] = True
    send("pane.report_agent", params)


def last_line(text):
    for line in reversed(str(text or "").splitlines()):
        if line.strip():
            return line.strip()
    return ""


def transcript_last_assistant_text():
    # Older Claude Code has no last_assistant_message: read the transcript's tail.
    path = hook_input.get("transcript_path")
    if not isinstance(path, str) or not path:
        return ""
    try:
        with open(path, "rb") as handle:
            handle.seek(0, os.SEEK_END)
            handle.seek(max(0, handle.tell() - 262144))
            lines = handle.read().decode("utf-8", "replace").splitlines()
    except Exception:
        return ""
    for line in reversed(lines):
        try:
            entry = json.loads(line)
        except Exception:
            continue
        if entry.get("type") != "assistant" or entry.get("isSidechain"):
            continue
        content = (entry.get("message") or {}).get("content")
        if isinstance(content, str):
            return content
        texts = [
            part.get("text", "")
            for part in content or []
            if isinstance(part, dict) and part.get("type") == "text"
        ]
        if texts:
            return "\n".join(texts)
    return ""


def subagents_running():
    tasks = hook_input.get("background_tasks")
    return isinstance(tasks, list) and any(
        isinstance(task, dict)
        and task.get("type") == "subagent"
        and task.get("status") == "running"
        for task in tasks
    )


if action == "session":
    if hook_event_name != "SessionStart" or is_subagent or not agent_session_id:
        raise SystemExit(0)
    params = {
        "pane_id": pane_id,
        "source": source,
        "agent": "claude",
        "seq": report_seq,
        "agent_session_id": agent_session_id,
    }
    transcript_path = hook_input.get("transcript_path")
    if isinstance(transcript_path, str) and transcript_path:
        params["agent_session_path"] = transcript_path
    session_start_source = hook_input.get("source")
    if isinstance(session_start_source, str) and session_start_source:
        params["session_start_source"] = session_start_source
    send("pane.report_agent_session", params)
    # A compaction can start inside a live turn; every other start waits at the prompt.
    if session_start_source in ("startup", "resume", "clear"):
        report_state("idle", report_seq + 1)
    raise SystemExit(0)

# A permission dialog raised for a sub-agent still waits in this pane.
if is_subagent and hook_event_name != "Notification":
    raise SystemExit(0)

if hook_event_name in ("UserPromptSubmit", "PostToolUse"):
    report_state("working", report_seq)
elif hook_event_name == "PreToolUse":
    if hook_input.get("tool_name") == "AskUserQuestion":
        report_state("blocked", report_seq, blocked_reason="form")
elif hook_event_name == "Notification":
    notification_type = hook_input.get("notification_type")
    if notification_type == "permission_prompt":
        report_state("blocked", report_seq, blocked_reason="permission")
    elif notification_type == "elicitation_dialog":
        report_state("blocked", report_seq, blocked_reason="form")
elif hook_event_name == "Stop":
    message = hook_input.get("last_assistant_message")
    if not isinstance(message, str):
        message = transcript_last_assistant_text()
    ending = last_line(message)
    if ending == QUESTION_SENTINEL:
        report_state("blocked", report_seq, blocked_reason="question")
    elif ending == BACKGROUND_SENTINEL or subagents_running():
        report_state("working", report_seq, background=True)
    else:
        report_state("idle", report_seq)
elif hook_event_name == "StopFailure":
    report_state("idle", report_seq)
PY
