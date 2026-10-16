# installed by herdr
# managed by herdr; reinstalling or updating the integration overwrites this file.
# add custom hooks beside this file instead of editing it.
# HERDR_INTEGRATION_ID=claude
# HERDR_INTEGRATION_VERSION=12

param([string]$Action = "")

if ($Action -ne "session" -and $Action -ne "state") { exit 0 }
if ($env:HERDR_ENV -ne "1") { exit 0 }
if ([string]::IsNullOrWhiteSpace($env:HERDR_PANE_ID)) { exit 0 }

# Read hook input as raw bytes so UTF-8 payloads decode correctly even when the
# console input code page is not UTF-8 (for example CP949 on Korean Windows).
$stdinReader = New-Object System.IO.StreamReader([Console]::OpenStandardInput(), [System.Text.Encoding]::UTF8)
$inputText = $stdinReader.ReadToEnd()
try {
    $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json }
} catch {
    exit 0
}

$propertyNames = @($payload.PSObject.Properties.Name)
if ((Test-Path Env:CURSOR_VERSION) -or $propertyNames -ccontains "cursor_version") { exit 0 }
if (-not ($propertyNames -ccontains "hook_event_name") -or $payload.hook_event_name -isnot [string]) { exit 0 }
$hookEvent = $payload.hook_event_name
$isSubagent = -not [string]::IsNullOrWhiteSpace($payload.agent_id)

$sessionId = $payload.session_id
if ([string]::IsNullOrWhiteSpace($sessionId)) { exit 0 }

$seq = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() * 1000000
$herdr = if ([string]::IsNullOrWhiteSpace($env:HERDR_BIN_PATH)) { "herdr" } else { $env:HERDR_BIN_PATH }

function Send-State([string]$State, [long]$Seq, [string]$BlockedReason = "", [bool]$Background = $false) {
    $stateArgs = @(
        "pane",
        "report-agent",
        $env:HERDR_PANE_ID,
        "--source",
        "herdr:claude",
        "--agent",
        "claude",
        "--state",
        $State,
        "--seq",
        "$Seq",
        "--agent-session-id",
        "$sessionId"
    )
    if (-not [string]::IsNullOrWhiteSpace($BlockedReason)) {
        $stateArgs += @("--blocked-reason", $BlockedReason)
    }
    if ($Background) {
        $stateArgs += @("--background")
    }
    try {
        & $herdr @stateArgs 2>$null | Out-Null
    } catch {
    }
}

function Get-LastLine([string]$Text) {
    $lines = @(([string]$Text) -split "`r?`n" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($lines.Count -eq 0) { return "" }
    return $lines[$lines.Count - 1].Trim()
}

function Get-TranscriptLastAssistantText {
    # Older Claude Code has no last_assistant_message: read the transcript's tail.
    $path = $payload.transcript_path
    if ($path -isnot [string] -or [string]::IsNullOrWhiteSpace($path) -or -not (Test-Path -LiteralPath $path)) { return "" }
    try {
        $lines = @(Get-Content -LiteralPath $path -Tail 400 -Encoding UTF8)
    } catch {
        return ""
    }
    for ($index = $lines.Count - 1; $index -ge 0; $index--) {
        try {
            $entry = $lines[$index] | ConvertFrom-Json
        } catch {
            continue
        }
        if ($entry.type -ne "assistant" -or $entry.isSidechain) { continue }
        $content = $entry.message.content
        if ($content -is [string]) { return $content }
        $texts = @($content | Where-Object { $_.type -eq "text" } | ForEach-Object { $_.text })
        if ($texts.Count -gt 0) { return ($texts -join "`n") }
    }
    return ""
}

if ($Action -eq "session") {
    if ($hookEvent -cne "SessionStart" -or $isSubagent) { exit 0 }
    try {
        $sessionArgs = @(
            "pane",
            "report-agent-session",
            $env:HERDR_PANE_ID,
            "--source",
            "herdr:claude",
            "--agent",
            "claude",
            "--seq",
            "$seq",
            "--agent-session-id",
            "$sessionId"
        )
        if ($payload.transcript_path -is [string] -and -not [string]::IsNullOrWhiteSpace($payload.transcript_path)) {
            $sessionArgs += @("--agent-session-path", "$($payload.transcript_path)")
        }
        if ($payload.source -is [string] -and -not [string]::IsNullOrWhiteSpace($payload.source)) {
            $sessionArgs += @("--session-start-source", "$($payload.source)")
        }
        & $herdr @sessionArgs 2>$null | Out-Null
    } catch {
    }
    # A compaction can start inside a live turn; every other start waits at the prompt.
    if (@("startup", "resume", "clear") -ccontains $payload.source) {
        Send-State "idle" ($seq + 1)
    }
    exit 0
}

# A permission dialog raised for a sub-agent still waits in this pane.
if ($isSubagent -and $hookEvent -cne "Notification") { exit 0 }

switch -CaseSensitive ($hookEvent) {
    "UserPromptSubmit" { Send-State "working" $seq }
    "PostToolUse" { Send-State "working" $seq }
    "PreToolUse" {
        if ($payload.tool_name -ceq "AskUserQuestion") { Send-State "blocked" $seq "form" }
    }
    "Notification" {
        if ($payload.notification_type -ceq "permission_prompt") { Send-State "blocked" $seq "permission" }
        elseif ($payload.notification_type -ceq "elicitation_dialog") { Send-State "blocked" $seq "form" }
    }
    "Stop" {
        $message = if ($payload.last_assistant_message -is [string]) { $payload.last_assistant_message } else { Get-TranscriptLastAssistantText }
        $ending = Get-LastLine $message
        $subagentsRunning = @($payload.background_tasks | Where-Object { $_.type -ceq "subagent" -and $_.status -ceq "running" }).Count -gt 0
        if ($ending -ceq "$([char]0x23F3) awaiting your answer") {
            Send-State "blocked" $seq "question"
        } elseif ($ending -ceq "$([char]::ConvertFromUtf32(0x1F504)) awaiting background work" -or $subagentsRunning) {
            Send-State "working" $seq "" $true
        } else {
            Send-State "idle" $seq
        }
    }
    "StopFailure" { Send-State "idle" $seq }
}
