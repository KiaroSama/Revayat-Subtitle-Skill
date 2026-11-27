# Verify a Revayat Subtitle release

The `v3.0.4` release workflow builds eight binaries: `revayat-subtitle` and `subsync` for Windows and Linux, each on x64 and arm64. Download the binary you need from the GitHub release, then run the commands from your download directory. The examples use x64 filenames; substitute the name of your download.

Copy the [Windows certificate](revayat-subtitle-codesign.cer) or [Ed25519 public key](revayat-subtitle-release-ed25519.pub.pem) from a repository revision you trust. A key downloaded alongside a binary cannot, by itself, tell you who signed that binary.

## Windows: Authenticode

In PowerShell, compare the signer's thumbprint with the trusted certificate and check for a timestamp. You do not need to import the certificate for this comparison.

```powershell
$certPath = (Resolve-Path -LiteralPath './revayat-subtitle-codesign.cer').Path
$cert = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($certPath)
$sig = Get-AuthenticodeSignature -LiteralPath './revayat-subtitle-v3.0.4-windows-x64.exe'
if (!$sig.SignerCertificate -or $sig.SignerCertificate.Thumbprint -ne $cert.Thumbprint -or
    !$sig.TimeStamperCertificate -or $sig.Status -notin 'Valid', 'UnknownError') {
    throw "Authenticode verification failed: $($sig.Status)"
}
$sig.Status
```

The project certificate is self-signed, so Windows can report `UnknownError` if it does not trust the certificate. That status is not a successful Windows trust check. Verify the signed checksum list or the GitHub attestation before running the file. A thumbprint mismatch, missing timestamp, `NotSigned`, or `HashMismatch` means you should stop.

## Linux: Ed25519

Place the matching `.sig` file next to the binary. With OpenSSL 3, run:

```bash
bin=revayat-subtitle-v3.0.4-linux-x64
openssl pkeyutl -verify -pubin -inkey revayat-subtitle-release-ed25519.pub.pem \
  -rawin -in "$bin" -sigfile "$bin.sig"
```

A zero exit code means the signature matches the file and the trusted key.

## SHA-256 checksums

Verify the signature on `SHA256SUMS.txt` before using it to check the downloads:

```bash
openssl pkeyutl -verify -pubin -inkey revayat-subtitle-release-ed25519.pub.pem \
  -rawin -in SHA256SUMS.txt -sigfile SHA256SUMS.txt.sig
```

If verification succeeds and you downloaded all eight binaries, run `sha256sum --check SHA256SUMS.txt` on Linux. To check one file, compare the output of `sha256sum "$bin"` with its line in the list. On Windows, use `(Get-FileHash -Algorithm SHA256 -LiteralPath './revayat-subtitle-v3.0.4-windows-x64.exe').Hash.ToLowerInvariant()`. If any hash differs, discard that download.

## GitHub build provenance

With the GitHub CLI installed and signed in, verify the binary against the release workflow's attestation:

```bash
gh attestation verify ./revayat-subtitle-v3.0.4-linux-x64 \
  --repo KiaroSama/Revayat-Subtitle-Skill \
  --signer-workflow KiaroSama/Revayat-Subtitle-Skill/.github/workflows/release.yml
```

Use the Windows `.exe` path to verify a Windows download. The attestation ties the exact file bytes to the GitHub Actions workflow and its source revision.
