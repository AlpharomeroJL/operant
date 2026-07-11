# Operant auto-continue launcher (fallback for session-limit / away-from-keyboard).
# Fired once by the Windows Scheduled Task "OperantAutoContinue". Survives the
# interactive Claude session dying. Guards against a double-run, then relaunches
# Claude headless in the repo to continue the campaign until v1.0.0 is released.
$ErrorActionPreference = 'Continue'
$repo   = 'D:\dev\operant'
$claude = 'C:\Users\jo312\AppData\Roaming\npm\claude.ps1'
$log    = Join-Path $repo 'campaign\auto-continue.log'
$prompt = Get-Content (Join-Path $repo 'campaign\continue-prompt.txt') -Raw

function Log($m) { "$([DateTimeOffset]::Now.ToString('u')) $m" | Tee-Object -FilePath $log -Append | Out-Null }

Set-Location $repo
Log "=== auto-continue fired ==="

# Concurrency guard: if main was pushed within the last 45 minutes, an interactive
# orchestrator is almost certainly still running. Skip, so we never double-run.
try { git fetch origin main --quiet 2>$null } catch {}
$lastCt = (git log -1 --format=%ct origin/main 2>$null)
if ($lastCt) {
  $age = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() - [int64]$lastCt
  if ($age -lt 2700) {
    Log "active session detected (last push ${age}s ago < 2700s). Skipping to avoid a double run."
    exit 0
  }
  Log "last push ${age}s ago; assuming the session ended. Continuing the campaign."
} else {
  Log "could not read origin/main; continuing anyway."
}

if (-not (Test-Path $claude)) { Log "claude CLI not found at $claude. Aborting."; exit 1 }

# Durable outer loop: each iteration is one headless work session. Stop when the
# release sentinel appears, or after a generous cap.
for ($i = 1; $i -le 24; $i++) {
  if (Test-Path (Join-Path $repo 'campaign\merged\RELEASED.ok')) {
    Log "RELEASED.ok present; campaign complete. Stopping."
    break
  }
  Log "--- continuation session $i ---"
  try {
    & $claude -p $prompt --dangerously-skip-permissions --model sonnet --add-dir $repo *>> $log
    Log "session $i exited (code $LASTEXITCODE)"
  } catch {
    Log "session $i error: $($_.Exception.Message)"
  }
  Start-Sleep -Seconds 60
}
Log "=== auto-continue finished ==="
