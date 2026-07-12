# Manual verification: uninstaller "remove saved data" prompt

Scope: prove that the NSIS `NSIS_HOOK_PREUNINSTALL` hook in
`release/nsis/installer-hooks.nsh` does exactly what it claims on a real
Windows install and uninstall:

- **Decline preserves.** Choosing No (the default) at the "remove saved data"
  prompt leaves both per-user data directories on disk.
- **Accept removes only that.** Choosing Yes removes exactly the two
  identifier-scoped data directories and nothing else.

These steps cannot be run in the build/CI environment: NSIS and `cargo tauri
build` are not installed here (see `release/REPRODUCIBLE.md`), so the hook has
never been compiled or executed. Run this on a clean Windows desktop against a
real installer produced by `just package` (or `cargo tauri build -b nsis`).

The two directories under test are keyed by the Tauri `identifier`
`dev.operant.shell` from `ui/src-tauri/tauri.conf.json`, confirmed against where
the app writes user data in `docs/specs/ipc-bridge.md` section 5:

| Directory | Tauri path | Holds |
| --- | --- | --- |
| `%APPDATA%\dev.operant.shell` | `app_config_dir()` / `app_data_dir()` | `config.json`, `recorder.sqlite3` (saved workflows and recordings) |
| `%LOCALAPPDATA%\dev.operant.shell` | `app_local_data_dir()` | WebView2 localStorage and cache |

## Prerequisites

1. A clean Windows 10/11 user account (no prior Operant data under `%APPDATA%`
   or `%LOCALAPPDATA%`).
2. A built installer, `Operant_<version>_x64-setup.exe`.
3. A PowerShell window. All snippets below are PowerShell.

## Seed helper (run before each uninstall scenario)

This creates the two real data directories with marker files, plus three
**decoy** directories that MUST survive an accept. The decoys prove the hook
removes *only* the two identifier-scoped folders:

- `%APPDATA%\Operant` is the old, wrong target the previous buggy build used.
- `%APPDATA%\dev.operant.shell.KEEP` is a same-prefix sibling; it must not be
  caught by a loose match.
- `%LOCALAPPDATA%\dev.operant.shell.KEEP` is the local-side sibling.

```powershell
$roam  = Join-Path $env:APPDATA      'dev.operant.shell'
$local = Join-Path $env:LOCALAPPDATA 'dev.operant.shell'
$decoyOld   = Join-Path $env:APPDATA      'Operant'
$decoyRoam  = Join-Path $env:APPDATA      'dev.operant.shell.KEEP'
$decoyLocal = Join-Path $env:LOCALAPPDATA 'dev.operant.shell.KEEP'

New-Item -ItemType Directory -Force $roam, $local, $decoyOld, $decoyRoam, $decoyLocal | Out-Null
Set-Content (Join-Path $roam  'config.json') '{"marker":"roaming"}'
Set-Content (Join-Path $local 'marker.txt')  'local'
Set-Content (Join-Path $decoyOld   'keep.txt') 'old wrong target, unrelated, must survive'
Set-Content (Join-Path $decoyRoam  'keep.txt') 'roaming sibling, must survive'
Set-Content (Join-Path $decoyLocal 'keep.txt') 'local sibling, must survive'
```

Instead of seeding, you may launch the installed app once and teach one
workflow; that populates the two real directories naturally. The decoys still
need creating by hand to prove the "nothing else" property.

## Scenario A: decline preserves

1. Install `Operant_<version>_x64-setup.exe` (per-user, the default).
2. Run the **Seed helper** above.
3. Start the uninstall: Settings > Apps > Installed apps > Operant > Uninstall
   (or run `uninstall.exe` from the install directory).
4. At the prompt **"Remove saved Operant workflows and recordings too?"**,
   confirm the default highlighted button is **No**, then choose **No**.
5. Let the uninstall finish.
6. Assert both data directories still exist:

```powershell
Test-Path (Join-Path $env:APPDATA      'dev.operant.shell')  # expect True
Test-Path (Join-Path $env:LOCALAPPDATA 'dev.operant.shell')  # expect True
```

**Pass:** both print `True`. The saved data was preserved on decline.

## Scenario B: accept removes only that

1. Reinstall `Operant_<version>_x64-setup.exe`.
2. Run the **Seed helper** above again (recreates the two dirs and the decoys).
3. Start the uninstall as in Scenario A.
4. At the prompt, choose **Yes**.
5. Let the uninstall finish.
6. Assert the two identifier-scoped directories are gone and every decoy
   survived:

```powershell
Test-Path (Join-Path $env:APPDATA      'dev.operant.shell')       # expect False
Test-Path (Join-Path $env:LOCALAPPDATA 'dev.operant.shell')       # expect False
Test-Path (Join-Path $env:APPDATA      'Operant')                 # expect True
Test-Path (Join-Path $env:APPDATA      'dev.operant.shell.KEEP')  # expect True
Test-Path (Join-Path $env:LOCALAPPDATA 'dev.operant.shell.KEEP')  # expect True
```

**Pass:** the first two print `False` and the last three print `True`. The hook
removed exactly the two identifier-scoped folders and nothing else.

## Optional: PATH cleanup (same hook file, uninstall side)

The same hook file adds the install directory to the per-user `PATH` on install
and removes it on uninstall. To confirm the uninstall side did not leave a
dangling entry, after either scenario:

```powershell
(Get-ItemProperty 'HKCU:\Environment' -Name Path).Path -split ';' |
  Where-Object { $_ -match 'Operant' }   # expect no output
```

**Pass:** no line is printed (the install directory was removed from `PATH`).

## Cleanup after testing

Remove any decoys and leftover data the scenarios created:

```powershell
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue `
  (Join-Path $env:APPDATA      'dev.operant.shell'),
  (Join-Path $env:LOCALAPPDATA 'dev.operant.shell'),
  (Join-Path $env:APPDATA      'Operant'),
  (Join-Path $env:APPDATA      'dev.operant.shell.KEEP'),
  (Join-Path $env:LOCALAPPDATA 'dev.operant.shell.KEEP')
```

## Recording the result

When both scenarios pass on a real installer, strike the uninstaller item fully
from `docs/KNOWN_ISSUES.md` and note the verified result in `CHANGELOG.md`.
Until then, both honestly state the fix is in code with this end-to-end check
pending.
