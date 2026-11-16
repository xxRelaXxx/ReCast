RECAST - USE YOUR ANDROID PHONE AS A WEBCAM AND MICROPHONE ON WINDOWS
=======================================================================

Thank you for using ReCast.

ReCast turns a spare phone into the only camera and microphone your PC needs.
No accounts, no ads, no telemetry. Video and audio travel directly between the phone
and the PC, over USB or your own Wi-Fi network. Nothing is uploaded anywhere.

WHAT IS IN THIS FOLDER
----------------------
  desktop\ReCast.exe         The desktop app, in a single file.
  phone\ReCastPhone.apk      The app to install on your Android phone.
  README.md                  The full documentation.
  USER-GUIDE.md              Step-by-step guide and troubleshooting.
  docs\                      Architecture and build notes, with the screenshots.
  LICENSE, THIRD_PARTY_NOTICES.md
                             The licences of ReCast and of the bundled components.

QUICK START
-----------
1. Run desktop\ReCast.exe.

   It installs itself into %LOCALAPPDATA%\ReCast, registers the virtual camera for
   your Windows user and starts the app. The first run asks once for administrator
   approval to remove an old camera entry and to allow the app through Windows
   Firewall. You can decline: the virtual camera still works for your user, and
   Windows asks about the network permission the first time the phone connects.

2. Copy phone\ReCastPhone.apk to the phone and open it there. Allow "Install
   unknown apps" if Android asks, and allow the camera and the microphone when the app
   starts. The APK is signed with a build key for easy sideloading; it is not on Google
   Play, so Play Protect may warn you once.

3. Connect the phone.

   Wi-Fi : the desktop app opens on a pairing screen with a QR code and a 6-digit
          PIN. In ReCast Phone, tap "Scan the QR code on your PC" and point the
          phone at the code. The app fills in the address and PIN and connects by
          itself. You can also type the address and PIN shown under the code.
          Both devices must be on the same network.

   USB  : on the phone, enable Developer options and USB debugging, connect a data
          cable and accept "Allow USB debugging?". Install the Android platform-tools
          so that adb.exe is in PATH, then press "USB setup" in ReCast. The app
          installs the APK, opens the tunnel and connects by itself.

4. In the program you want to use - Zoom, Discord, OBS, a browser, Teams... - choose
   "ReCast Virtual Camera" as the camera.

5. For the microphone, install a virtual audio cable (for example Voice.ai Audio
   Cable), enable "Virtual microphone" in ReCast and pick the capture side of the
   cable in the other program.

NEED MORE HELP?
---------------
  USER-GUIDE.md      connection, controls, troubleshooting, removal
  README.md          project overview and build instructions
  Issues:           https://github.com/xxRelaXxx/ReCast/issues

PRIVACY
-------
The phone connects directly to the PC. Video and audio never leave your local network
or your USB link. ReCast has no accounts, no ads, no analytics and no network access
other than the local stream. The local link is not encrypted, so use it on a network
you trust.

ReCast is free software under the MIT licence.
