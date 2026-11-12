# Rewebcam user guide

This guide covers the whole journey: installing the app, connecting the phone, using the
virtual camera and microphone, and fixing the problems that show up most often.

---

## 1. What you need

* Windows 10 or 11, 64-bit.
* An Android 9 (API 29) or newer phone with a camera.
* A USB data cable (recommended) or the same Wi-Fi network on both devices.
* Optional, for the virtual microphone: a virtual audio cable, for example the free
  *Voice.ai Audio Cable*. Rewebcam sends the phone audio into that cable; other apps record
  from its microphone side.

---

## 2. Installing

The release archive contains two ready-to-use positions:

```text
ReWebcam-1.0.0/
├── desktop/
│   └── Rewebcam.exe       the desktop app, in one file
└── phone/
    └── RewebcamPhone.apk  the phone app
```

### The desktop app

`desktop/Rewebcam.exe` is a self-contained launcher. On the first run it:

1. unpacks the runtime into `%LOCALAPPDATA%\Rewebcam`;
2. registers the virtual camera **for your Windows user** (no administrator rights
   needed for this);
3. offers a one-time administrator approval that removes an old, machine-wide
   "DirectShow Softcam" entry left by an earlier build and adds a Windows Firewall rule
   for TCP port 47777;
4. starts the app.

You can decline the administrator prompt. The virtual camera then stays registered for
your user only, and Windows asks about the network permission the first time the phone
connects. Choose **Private networks** when it does.

Later launches reuse the installed copy, so you can pin the launcher to the taskbar or put
it in the Startup folder. Running it twice does not open two apps: the second run brings
the existing window to the front.

### The phone app

Copy `phone/RewebcamPhone.apk` to the phone and open it. If Android asks, allow
"Install unknown apps" for the app you are opening it with. The APK is signed with a local
build key so that it can be sideloaded without extra tooling; it is not published on Google
Play, so Play Protect may warn about an unknown developer. Choose "Install anyway" if you
trust the file you downloaded.

The phone app requests camera and microphone access when it starts. It needs both.

---

## 3. Connecting the phone

### Wi-Fi

1. Start `desktop/Rewebcam.exe`.
2. The **Connection** panel shows an address such as `192.168.1.20:47777` and a 6-digit
   PIN. Both have a *Copy* button.
3. On the phone, open Rewebcam, enter the address and the PIN, and press **Connect**.
4. The status line turns green and shows the resolution, frame rate, bitrate and battery.

Keep both devices on the same home network. Guest Wi-Fi with client isolation blocks the
connection; use USB or a phone hotspot in that case.

### USB

USB is faster and does not depend on the network.

1. On the phone: *Settings > About phone > tap "Build number" seven times*.
2. *Settings > System > Developer options > enable "USB debugging"*.
3. Connect the USB cable, choose "File transfer" if Android asks, and accept "Allow USB
   debugging?" on the phone.
4. Install the Android platform-tools so that `adb.exe` is in `PATH` or in
   `%LOCALAPPDATA%\Android\Sdk\platform-tools`.
5. Press **USB setup** in Rewebcam. The app installs the APK, opens the reverse tunnel
   (`adb reverse tcp:47777`) and starts the phone app with the address and PIN filled in.

If you prefer, you can install the APK by hand and simply type `127.0.0.1:47777` as the
address in the phone app — the tunnel is the only thing that makes the PC reachable over
USB.

---

## 4. Using the virtual camera

Open the application you want to use (Zoom, Discord, OBS, Chrome, Edge, Teams, a browser
site…) and choose **Rewebcam Virtual Camera** in its camera list.

* If the camera is missing, close the other application completely and open it again so
  that it re-reads the device list.
* If the camera was selected before, pick it again once: applications remember their
  choice and do not always re-enumerate.
* 32-bit applications use the x86 filter, 64-bit applications the x64 filter. Both are
  installed.

### Using the virtual microphone

Install a virtual audio cable (for example *Voice.ai Audio Cable*), then:

1. enable **Virtual microphone** in Rewebcam;
2. in the other application, select the **capture** side of the cable — for example
   *Microphone (Voice.ai Audio Cable)*, not *Speakers (Voice.ai Audio Cable)*;
3. check the **Mic** meter in Rewebcam: it shows the level that is being sent.

Rewebcam detects the most common cable names automatically (Voice.ai Audio Cable, VB-CABLE,
Cable Input, DroidCam Audio, Steam Streaming Speakers). If your cable is not detected,
pick it manually under **Devices**.

### OBS Studio

* Add a **Video Capture Device** source and select *Rewebcam Virtual Camera*.
* Add an **Audio Input Capture** source and select the capture side of your virtual cable
  if you want the phone microphone in the recording.

### Browsers and websites

1. Start Rewebcam, connect the phone and keep **Virtual camera** enabled.
2. Open the site and allow camera and microphone when it asks.
3. Choose *Rewebcam Virtual Camera* in the site's own camera menu. A site that still has
   an old choice such as *OBS Virtual Camera* will show nothing.
4. Reload the page if it was opened before the phone was connected.

Some sites request the camera and the microphone in one step and show nothing when either
one is denied. In Windows, check *Settings > Privacy & security > Microphone* and
*Camera*, and enable "Let desktop apps access your microphone/camera".

---

## 5. Controls

### Connection

| Control | What it does |
| --- | --- |
| Address | The PC address the phone has to connect to. |
| PIN | The 6-digit pairing code. It changes every time Rewebcam starts. |
| USB setup | Installs the phone app over USB, opens the tunnel and connects automatically. |

### Camera

| Control | What it does |
| --- | --- |
| Camera | Switch between the phone's front, back and additional cameras. The phone app follows the same choice. |
| Resolution | Pick one of the resolutions the selected camera really supports. 1920x1080 or 1280x720 at 30 fps is a good default; smaller resolutions use less CPU and bandwidth. |
| Frame rate | Available when the selected resolution offers several rates. |
| Phone microphone | Mutes or unmutes the phone audio. |
| Torch | Turns the phone flashlight on or off while the back camera is active. |
| Zoom | Digital zoom from 1x to 8x. |
| Rotation +90° | Adds a quarter turn to the stream. The output switches between 1920x1080 and 1080x1920 so that other applications always get an upright picture. |
| Reset | Goes back to the orientation the phone reports. |
| Mirror preview | Flips the preview only. Useful for the front camera. Other applications always get the unmirrored image unless the phone itself reports a mirrored stream. |

### Output

**Virtual camera** enables or disables the camera that other applications see. *Ready*
means the filter is registered and waiting, *In use* means an application has opened it.

### Audio

| Control | What it does |
| --- | --- |
| Virtual microphone | Sends the phone audio to the virtual cable. |
| Monitor on speakers | Plays the phone audio on the PC speakers. Keep the phone away from the speakers to avoid feedback. |
| Mic | Level meter of the outgoing audio. |
| Devices | Choose the monitor and virtual-cable outputs, set the mic and monitor levels, and rescan after plugging in a cable. |

### Noise suppression

RNNoise removes steady background noise. **Strength** mixes between the raw audio (0) and
fully cleaned audio (100); the default is on at 80. It applies to the virtual microphone
and to the speaker monitor. Very high settings can remove quiet speech together with the
noise.

### Theme

Mono (black and white, default), Graphite, Light, or custom colours. The theme applies to
the current session.

---

## 6. The phone app

* **PC address**, **port** and **PIN** are on the right. The address and PIN are remembered
  between runs.
* **Camera** and **Resolution** mirror the desktop controls, and both stay in sync
  whichever side changes them.
* **Microphone** and **Torch** are switches, **Zoom** is a slider.
* The preview is rotated and cropped so it always fills its frame without stretching, and
  it follows the phone when you turn it. Turning the phone recreates only the screen: the
  stream keeps running and the app reconnects to it automatically.
* The status line shows the connection state, the resolution, the frame rate, the bitrate,
  the battery level and whether the microphone is on.

---

## 7. Troubleshooting

### No video

* Check that the phone shows "Connected" and the foreground-service notification.
* The message under the connection panel explains most failures.
* Try USB, or a phone hotspot if the Wi-Fi isolates clients.
* Close other virtual camera software. Only one program can own the camera filter at a
  time, so DroidCam and Rewebcam cannot run together.

### A website shows no picture

* Pick *Rewebcam Virtual Camera* in the site's own camera selector.
* Allow both camera and microphone.
* Check *Windows Settings > Privacy & security > Microphone* and *Camera*.
* Reload the page after Rewebcam is connected, or reset the site's camera permission in
  the browser (padlock icon > *Site settings*) and allow it again.

### The picture is sideways or upside down

* Press **+90°** in Rewebcam until the picture is upright. Some phones report the wrong
  camera orientation, especially after rotating the phone.
* Press **Reset** to return to the orientation the phone reports.

### The camera is not listed in another application

* Pick *Rewebcam Virtual Camera* again in that application.
* Close and reopen the application so it re-reads the device list.
* 32-bit applications use the x86 filter, 64-bit applications the x64 filter; both are
  installed by the launcher.

### The microphone is silent

* Install a virtual audio cable and check that Windows shows both its playback and its
  recording endpoints.
* In the other application select the **capture** side of the cable.
* Unmute *Phone microphone* and enable *Virtual microphone* in Rewebcam.
* Check the *Mic* meter and raise *Mic level* under **Devices** if it stays low.

### The speech is cut off or sounds watery

* Lower the RNNoise strength, or switch RNNoise off. Strong suppression removes quiet
  speech together with the noise.

### The connection fails

* The PC and the phone must be on the same network, and the address shown must be
  reachable from the phone.
* Allow `rewebcam.exe` through Windows Firewall for private networks, or accept the
  administrator prompt on the first launch.
* The PIN must match the one on screen.

### USB setup cannot find the phone

* Install the Android platform-tools and make sure `adb.exe` is in `PATH`.
* Accept the "Allow USB debugging?" prompt on the phone.
* Use a data cable, not a charge-only cable.

### Windows warns about an unknown application

The binaries are not code-signed. Choose *More info > Run anyway* in the SmartScreen
dialog, or sign `Rewebcam.exe` with your own certificate.

---

## 8. Removing Rewebcam

1. Close Rewebcam.
2. Delete `%LOCALAPPDATA%\Rewebcam`.
3. Optional: remove the virtual camera registration and the firewall rule:

   ```powershell
   reg delete "HKCU\Software\Classes\CLSID\{8E2C0D5B-3F5A-4D7E-9B2C-6A1D4F7E9C31}" /f
   reg delete "HKCU\Software\Classes\CLSID\{AEF3B972-5FA5-4647-9571-358EB472BC9E}" /f
   reg delete "HKCU\Software\Classes\CLSID\{860BB310-5D01-11D0-BD3B-00A0C911CE86}\Instance\Rewebcam Virtual Camera" /f
   netsh advfirewall firewall delete rule name=Rewebcam
   ```

   Run the commands in an elevated PowerShell if you want the machine-wide entries removed
   as well.

4. Uninstall the phone app from the phone's app settings.

---

## 9. Privacy

The phone connects directly to the PC. Video and audio never leave your local network or
your USB link. Rewebcam has no accounts, no ads, no analytics and no network access other
than the local video and audio stream. The local link is not encrypted, so use it on a
network you trust.
