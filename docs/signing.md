# Code signing (Authenticode)

This is the setup guide for OS-level code signing of the Windows NSIS
installer, `Operant_<version>_x64-setup.exe`. It is a different mechanism
from the auto-updater's Ed25519 signature described in `release/KEYS.md`:

- **Updater signing (Ed25519, `release/scripts/updater-keys.mjs`)**: proves
  an update artifact really came from this project before the app installs
  it. Already implemented and required.
- **Authenticode signing (this document)**: proves to Windows itself, before
  the app is even installed, that the installer's publisher is known. This
  is what silences the SmartScreen "unknown publisher" warning described in
  `docs/install.md` and `docs/KNOWN_ISSUES.md`. Not yet in place, because no
  certificate has been purchased.

Nothing in this document is required for the app to work correctly or
safely; it only removes a warning dialog and a click. Treat it as launch
polish, not a blocker.

## What you need

An Authenticode code-signing certificate, in one of two shapes:

- A certificate installed in your Windows certificate store, identified by
  its SHA1 thumbprint.
- A certificate exported as a `.pfx`/`.p12` file plus its password.

Either shape works with `just sign` (see "Configuring `just sign`" below).
There are two ways to get one.

## Option A: Azure Trusted Signing (recommended)

Azure Trusted Signing (Microsoft renamed it "Azure Artifact Signing" in
early 2026; same service) is a cloud-hosted signing certificate: Microsoft
validates your identity once, then holds the private key in an HSM you
never touch directly. `signtool` calls out to it per signature instead of
reading a local key. This avoids buying and shipping around a physical EV
USB token, and it is the cheapest path to a certificate that a fresh
project can get today.

**Approximate cost** (Azure list pricing as of mid-2026; check the
[Azure Artifact Signing pricing page](https://azure.microsoft.com/en-us/pricing/details/artifact-signing/)
for current numbers before budgeting):

- Basic tier: **$9.99/month**, up to 5,000 signatures/month, one
  certificate profile. Plenty for a single-project installer.
- Premium tier: **$99.99/month**, up to 100,000 signatures/month, plus
  $0.005 per signature beyond that, and more certificate profiles.

**Setup steps:**

1. In the Azure portal, create a **Trusted Signing** (Artifact Signing)
   account in a supported region (generally available in the US, Canada,
   and Europe as of January 2026).
2. Complete **identity validation** for the account. Individual developers
   can now enroll (public preview opened this to solo developers, not just
   established organizations); this step is the main source of delay,
   budget a few business days.
3. Create a **certificate profile** of type "Public Trust" under the
   account.
4. Assign yourself (or the account that will run `just sign`) the
   **Trusted Signing Certificate Profile Signer** role on that profile
   (Azure RBAC, `Access control (IAM)` on the certificate profile).
5. On the signing machine, install:
   - **.NET 8.0 Runtime** (the signing plugin below requires it).
   - **signtool.exe** version 10.0.22621.755 or newer (see "Locating
     signtool.exe" below; older SDKs will not work with Trusted Signing).
   - The **Trusted Signing / Artifact Signing dlib package** (from the
     Microsoft.Trusted.Signing.Client NuGet package, or the standalone
     installer linked from the
     [signing integrations doc](https://learn.microsoft.com/en-us/azure/artifact-signing/how-to-signing-integrations)).
     Note the path to `Azure.CodeSigning.Dlib.dll` inside it.
6. Write a metadata JSON file describing your account and profile, e.g.:

   ```json
   {
     "Endpoint": "https://<region>.codesigning.azure.net",
     "CodeSigningAccountName": "<your-account-name>",
     "CertificateProfileName": "<your-profile-name>"
   }
   ```

7. Sign in with an identity that holds the RBAC role above (`az login`, or
   a service principal with its credentials in the environment for
   non-interactive use), then sign directly with `signtool`:

   ```
   signtool.exe sign /v /fd SHA256 /tr "http://timestamp.acs.microsoft.com" /td SHA256 ^
     /dlib "<path-to-dlib>\x64\Azure.CodeSigning.Dlib.dll" ^
     /dmdf "<path-to-metadata>\metadata.json" ^
     path\to\Operant_<version>_x64-setup.exe
   ```

**Known gap:** `scripts/sign.ps1` (what `just sign` runs) only implements
the two local-key modes in "Configuring `just sign`" below
(`OPERANT_SIGN_THUMBPRINT` and `OPERANT_SIGN_PFX`). Trusted Signing keys
never leave Azure, so there is no thumbprint in `Cert:\CurrentUser\My` and
no PFX file to point at; it needs the separate `/dlib` + `/dmdf` invocation
above. Until `scripts/sign.ps1` grows a third mode for this (a natural
extension point: an `OPERANT_SIGN_AZURE_METADATA_JSON` variable that
triggers the `/dlib`/`/dmdf` arguments instead of `/sha1` or `/f`/`/p`),
sign with the `signtool` command above directly rather than through
`just sign` if you choose this option. This is a known, tracked gap, not a
silent one.

## Option B: Traditional OV or EV certificate

The older, still very common path: buy a certificate from a public
certificate authority (DigiCert, Sectigo, SSL.com, GlobalSign, and several
resellers all sell these).

- **OV (Organization Validated)**: the CA verifies your organization exists
  (or, for some CAs, verifies an individual) and issues a certificate you
  can export as a `.pfx` and use directly with `OPERANT_SIGN_PFX`, or
  import into your certificate store and use with
  `OPERANT_SIGN_THUMBPRINT`. **Approximate cost: $215-$390/year**
  depending on CA and reseller.
- **EV (Extended Validation)**: stricter identity vetting, and the private
  key must live on a **FIPS-compliant hardware token** (a USB device) or an
  approved cloud HSM; it cannot be exported as a plain `.pfx`. Historically
  EV certificates got installers past SmartScreen's reputation check
  faster than OV; that gap has narrowed since Microsoft now builds
  reputation for any signed, consistently-published binary, but EV is
  still the stricter option some distribution channels expect.
  **Approximate cost: $325+/year**, roughly 30-40% more than the
  equivalent OV product; some CAs bundle the hardware token in that price,
  others charge for it separately. As of February 23, 2026 the CA/B Forum
  shortened the maximum code-signing certificate lifetime to about 15
  months (459 days), so a "multi-year" purchase now means a new
  certificate (and, for EV, typically a new token) issued each year rather
  than one long-lived certificate.

**Setup steps (either OV or EV):**

1. Choose a CA or reseller and buy an OV or EV code-signing certificate.
2. Complete identity validation (OV: organization or individual
   verification, typically a few business days; EV: stricter, may include
   a phone call and legal-entity paperwork).
3. Receive the certificate:
   - **OV**: usually issued as a downloadable `.pfx`/`.p12` file (or a
     browser-generated key you export as one).
   - **EV**: usually shipped as a hardware USB token, or provisioned into
     a CA-hosted cloud HSM, depending on vendor.
4. Make the certificate usable by `just sign`:
   - **PFX file**: keep it somewhere outside the repository (never commit
     a `.pfx`) and point `OPERANT_SIGN_PFX` at it.
   - **Hardware token**: install the token's PKCS#11/CSP drivers, plug it
     in, and it registers its certificate into
     `Cert:\CurrentUser\My` automatically. Find its thumbprint with:

     ```
     Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert | Format-List Subject, Thumbprint
     ```

     and set `OPERANT_SIGN_THUMBPRINT` to that value.
   - **PFX imported into the store** (an alternative to pointing at the
     file directly): `Import-PfxCertificate -FilePath .\cert.pfx
     -CertStoreLocation Cert:\CurrentUser\My -Password (ConvertTo-SecureString
     -String "..." -AsPlainText -Force)`, then use its thumbprint the same
     way.

## Configuring `just sign`

`just sign` (backed by `scripts/sign.ps1`) reads these environment
variables. Set them in your shell before running a release build, or in
whatever local, un-committed environment file your shell loads.

| Variable | Required | Purpose |
|---|---|---|
| `OPERANT_SIGN_THUMBPRINT` | One of this or `OPERANT_SIGN_PFX` | SHA1 thumbprint of a certificate in `Cert:\CurrentUser\My`. Takes priority if both are set. |
| `OPERANT_SIGN_PFX` | One of this or `OPERANT_SIGN_THUMBPRINT` | Path to a `.pfx`/`.p12` file. |
| `OPERANT_SIGN_PFX_PASSWORD` | Required whenever `OPERANT_SIGN_PFX` is set | Password for that file. An empty string is accepted if the PFX truly has none, but the variable must exist. |
| `OPERANT_SIGN_TIMESTAMP_URL` | No | RFC3161 timestamp server. Defaults to a public DigiCert server. Set to an empty string to skip timestamping (offline testing only; real releases should always timestamp). |

Neither `OPERANT_SIGN_THUMBPRINT` nor `OPERANT_SIGN_PFX` set: `just sign`
prints `sign: no signing certificate configured; shipping unsigned, see
docs/signing.md` and exits 0. This is a deliberate, clean skip so a release
build never fails just because signing was not set up; see
`docs/KNOWN_ISSUES.md` for the resulting SmartScreen warning and
`docs/install.md` for what a user does about it.

`just sign` also skips cleanly, with a message naming the requirement, if a
certificate **is** configured but `signtool.exe` cannot be found anywhere
(checked in order: `PATH`, then
`C:\Program Files (x86)\Windows Kits\10\bin\<version>\x64\signtool.exe`,
then `vswhere.exe`). Install the Windows SDK (the "Windows SDK Signing
Tools for Desktop Apps" component is enough on its own, if you do not want
the rest of the SDK) to fix that.

`just sign` with no arguments signs the newest `*.exe` under
`$CARGO_TARGET_DIR/release/bundle/nsis/` (where `cargo tauri build -b nsis`
writes the installer; `CARGO_TARGET_DIR` defaults to
`D:/dev/operant-target`, per the root `justfile`). Pass an explicit file
with `just sign -Path path\to\file.exe` to sign something else, such as a
throwaway test binary.

`just package` runs the full NSIS build (`release/REPRODUCIBLE.md`'s
"one-command rebuild") and then calls `just sign`, so a normal release
build signs automatically whenever a certificate is configured, and
produces a clearly-labeled unsigned installer otherwise.

## Verifying a signature

```
signtool verify /pa /v path\to\Operant_<version>_x64-setup.exe
```

`/pa` uses the same default Authenticode policy Windows itself applies
(rather than the narrower legacy driver-signing policy `signtool verify`
uses without it); `/v` prints the full certificate chain. A successful
verification against a real, CA-issued certificate ends with `Successfully
verified`. Signing with a self-signed test certificate (see below) is
expected to still show the signature itself as well-formed, but the trust
chain will report that it terminates in a root that Windows does not trust
by default; that is correct behavior for a self-signed certificate, not a
bug in `just sign` or in `signtool`.

## Testing the pipeline with a self-signed certificate

You can exercise the entire `just sign` path (everything except real
Windows/SmartScreen trust) without buying anything, using a throwaway
self-signed certificate:

```powershell
$cert = New-SelfSignedCertificate -Type CodeSigningCert `
  -Subject "CN=Operant Test Signing" `
  -CertStoreLocation Cert:\CurrentUser\My

$env:OPERANT_SIGN_THUMBPRINT = $cert.Thumbprint
just sign -Path path\to\some-throwaway.exe

signtool verify /pa /v path\to\some-throwaway.exe
```

Delete the test certificate afterward
(`Remove-Item Cert:\CurrentUser\My\$($cert.Thumbprint)`) since it is not a
real credential and should not linger in your certificate store.

## Related documents

- `docs/install.md`: what a user sees and does about the unsigned-installer
  SmartScreen warning today.
- `docs/KNOWN_ISSUES.md`: the honest, user-facing list this warning is
  tracked on.
- `release/KEYS.md`: the separate Ed25519 updater-signing mechanism.
- `release/REPRODUCIBLE.md`: how the installer itself is built.
