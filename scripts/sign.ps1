<#
.SYNOPSIS
    Authenticode-signs the built Operant NSIS installer with signtool.exe.

.DESCRIPTION
    This is the implementation behind `just sign`. It looks for a signing
    certificate identified by environment variables, locates signtool.exe
    (which ships with the Windows SDK and is not always on PATH), and runs
    the sign command.

    If no certificate is configured, or signtool.exe cannot be found, this
    prints a clear message and exits 0: a clean skip, not a failure.
    Shipping an unsigned installer is a supported, honest state for this
    project today; see docs/signing.md and docs/KNOWN_ISSUES.md.

.PARAMETER Path
    One or more files to sign. Defaults to the newest *.exe under
    "$env:CARGO_TARGET_DIR/release/bundle/nsis" (the NSIS bundle directory
    that `cargo tauri build -b nsis` writes to; CARGO_TARGET_DIR defaults to
    D:/dev/operant-target, matching the root justfile).

.PARAMETER TimestampUrl
    RFC3161 timestamp server URL. Defaults to $env:OPERANT_SIGN_TIMESTAMP_URL
    if set, else a public DigiCert timestamp server. Pass -TimestampUrl ""
    (or set OPERANT_SIGN_TIMESTAMP_URL to an empty string) to sign without a
    timestamp, e.g. for offline testing. Real releases should always be
    timestamped so the signature stays valid after the certificate expires.

.NOTES
    Certificate sources, checked in this order:

      1. OPERANT_SIGN_THUMBPRINT: SHA1 thumbprint of a certificate already
         in the current user's "My" store (Cert:\CurrentUser\My). This also
         covers an EV certificate on a hardware token once its drivers are
         installed and the token is plugged in, since the token registers
         its certificate into that same store.
      2. OPERANT_SIGN_PFX (+ OPERANT_SIGN_PFX_PASSWORD): path to a PFX/P12
         file and its password.
      3. Neither set: no certificate configured, clean skip.

    Azure Trusted Signing (see docs/signing.md) is not one of these two
    modes: it signs through a signtool plugin (/dlib + /dmdf against a
    cloud-held key; there is no local thumbprint or PFX file to point at),
    so it needs a third code path this script does not implement yet. Sign
    manually with the command in docs/signing.md until that lands.
#>
param(
    [string[]]$Path,
    [string]$TimestampUrl
)

function Write-Err {
    param([string]$Message)
    [Console]::Error.WriteLine($Message)
}

function Fail {
    param([string]$Message, [int]$Code = 1)
    Write-Err $Message
    exit $Code
}

function Find-Installer {
    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "D:/dev/operant-target" }
    $bundleDir = Join-Path $targetDir "release/bundle/nsis"
    if (-not (Test-Path $bundleDir)) { return $null }
    $exe = Get-ChildItem -Path $bundleDir -Filter "*.exe" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($exe) { return $exe.FullName }
    return $null
}

function Find-SignTool {
    # 1. Already on PATH.
    $onPath = Get-Command "signtool.exe" -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }

    # 2. Windows Kits install layout: .../Windows Kits/10/bin/<version>/x64/signtool.exe
    $pf86 = ${env:ProgramFiles(x86)}
    if ($pf86) {
        $kitsRoot = Join-Path $pf86 "Windows Kits\10\bin"
        if (Test-Path $kitsRoot) {
            $versioned = Get-ChildItem -Path $kitsRoot -Directory -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -match '^\d+(\.\d+){2,3}$' } |
                Sort-Object { [version]($_.Name) } -Descending
            foreach ($dir in $versioned) {
                $candidate = Join-Path $dir.FullName "x64\signtool.exe"
                if (Test-Path $candidate) { return $candidate }
            }
            $flat = Join-Path $kitsRoot "x64\signtool.exe"
            if (Test-Path $flat) { return $flat }
        }

        # 3. vswhere: signtool can also ship inside a Visual Studio install.
        $vswhere = Join-Path $pf86 "Microsoft Visual Studio\Installer\vswhere.exe"
        if (Test-Path $vswhere) {
            $found = & $vswhere -latest -prerelease -products * -find "**\signtool.exe" 2>$null |
                Where-Object { $_ -like "*\x64\signtool.exe" } |
                Select-Object -First 1
            if ($found) { return $found }
        }
    }

    return $null
}

# --- 1. Is a certificate configured at all? ---------------------------------
$thumbprint = $env:OPERANT_SIGN_THUMBPRINT
$pfxPath = $env:OPERANT_SIGN_PFX

if ([string]::IsNullOrEmpty($thumbprint) -and [string]::IsNullOrEmpty($pfxPath)) {
    Write-Host "sign: no signing certificate configured; shipping unsigned, see docs/signing.md"
    exit 0
}

$havePfxPassword = $null -ne [Environment]::GetEnvironmentVariable("OPERANT_SIGN_PFX_PASSWORD")
$pfxPassword = $env:OPERANT_SIGN_PFX_PASSWORD

if ([string]::IsNullOrEmpty($thumbprint) -and -not [string]::IsNullOrEmpty($pfxPath) -and -not $havePfxPassword) {
    Fail "sign: OPERANT_SIGN_PFX is set but OPERANT_SIGN_PFX_PASSWORD is not. Set the password (an empty string is fine if the PFX truly has none), or unset OPERANT_SIGN_PFX and set OPERANT_SIGN_THUMBPRINT instead."
}

# --- 2. Can we find signtool.exe? --------------------------------------------
$signtool = Find-SignTool
if (-not $signtool) {
    Write-Host "sign: a certificate is configured but signtool.exe was not found (requires the Windows SDK; checked PATH, Windows Kits, and vswhere.exe). Shipping unsigned, see docs/signing.md"
    exit 0
}

# --- 3. What are we signing? --------------------------------------------------
$targets = @()
if ($Path) {
    $targets = $Path
} else {
    $auto = Find-Installer
    if ($auto) { $targets = @($auto) }
}

if (-not $targets -or $targets.Count -eq 0) {
    $defaultDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "D:/dev/operant-target" }
    Fail "sign: nothing to sign. Pass -Path <file>, or build the installer first (see release/REPRODUCIBLE.md). Looked under $defaultDir/release/bundle/nsis/*.exe"
}

foreach ($t in $targets) {
    if (-not (Test-Path $t)) {
        Fail "sign: file not found: $t"
    }
}

# --- 4. Sign. -----------------------------------------------------------------
if ($PSBoundParameters.ContainsKey("TimestampUrl")) {
    $timestampUrl = $TimestampUrl
} elseif ($env:OPERANT_SIGN_TIMESTAMP_URL) {
    $timestampUrl = $env:OPERANT_SIGN_TIMESTAMP_URL
} else {
    $timestampUrl = "http://timestamp.digicert.com"
}

if (-not [string]::IsNullOrEmpty($thumbprint)) {
    $certArgs = @("/sha1", $thumbprint, "/s", "My")
    $mode = "thumbprint $thumbprint (Cert:\CurrentUser\My)"
} else {
    $certArgs = @("/f", $pfxPath, "/p", $pfxPassword)
    $mode = "PFX $pfxPath"
}

$signArgs = @("sign", "/fd", "SHA256") + $certArgs
if (-not [string]::IsNullOrEmpty($timestampUrl)) {
    $signArgs += @("/tr", $timestampUrl, "/td", "SHA256")
}

Write-Host "sign: using $mode via $signtool"

foreach ($file in $targets) {
    Write-Host "sign: signing $file"
    & $signtool @signArgs $file
    if ($LASTEXITCODE -ne 0) {
        Fail "sign: signtool exited $LASTEXITCODE while signing $file" $LASTEXITCODE
    }
}

Write-Host "sign: OK, signed $($targets.Count) file(s)"
exit 0
