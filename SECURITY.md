# Security policy

## Supported versions

| Version | Supported |
| --- | --- |
| 1.0.x | yes |
| older | no |

## Reporting a vulnerability

Please report security issues privately by opening a
[GitHub security advisory](https://github.com/xxRelaXxx/ReWebCam/security/advisories/new)
for this repository instead of opening a public issue. Include a description, the affected
version and, if possible, the steps to reproduce it.

You can expect an acknowledgement within a few days. Fixes for confirmed issues are
released as a new patch version, and the advisory is published together with the fix.

## What Rewebcam does and does not do

* Rewebcam only opens one TCP port, **47777**, for the incoming video and audio stream.
* The stream is **not encrypted**. Anyone who can reach the port and knows the current
  6-digit PIN can send video to the PC. Use it on a network you trust, and keep the
  pairing PIN secret.
* The PIN changes every time the desktop app starts. It is not stored anywhere except in
  memory.
* Rewebcam has no accounts, no telemetry and no network access other than the local
  stream.
* The launcher asks for administrator rights **once**, and only to remove an old
  machine-wide camera entry and to add a Windows Firewall rule for TCP port 47777.
  Everything else happens in the per-user registry under `HKCU`.
* The phone app requests only the camera, the microphone, internet access and the
  foreground-service permissions it needs. It has no other permissions and no analytics
  SDK.
* The binaries are not code-signed. Windows SmartScreen may therefore warn about an
  unknown publisher on the first run.

## Hardening your own installation

* Use the USB connection when the network is not trusted.
* Decline the administrator prompt if you do not want the firewall rule; the virtual
  camera still works for your user.
* Block the incoming rule in Windows Firewall if you only ever use USB.
