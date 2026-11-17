# ReCast user guide

This guide covers the whole journey: installing the app, connecting the phone, using the
virtual camera and microphone, and fixing the problems that show up most often.

---

## 1. What you need

* Windows 10 or 11, 64-bit.
* An Android 9 (API 29) or newer phone with a camera.
* A USB data cable (recommended) or the same Wi-Fi network on both devices.
* Optional, for the virtual microphone: a virtual audio cable, for example the free
  *Voice.ai Audio Cable*. ReCast sends the phone audio into that cable; other apps record
  from its microphone side.

---

## 2. Installing

The release archive contains two ready-to-use positions:

```text
ReCast-1.0.0/
├── desktop/
│   └── ReCast.exe       the desktop app, in one file
└── phone/
    └── ReCastPhone.apk  the phone app
```

### The desktop app

`desktop/ReCast.exe` is a self-contained installer. It opens a setup window in the
same black and white style as the app. Drag the title band to move it, and drag an
edge to resize it. The window walks through three steps:

1. **Where should ReCast live?** The suggestion is `%LOCALAPPDATA%\ReCast`. Any
   folder on any drive works, including `X:\ReCast`. **Browse** opens the Windows
   folder dialog, which lists every drive. The drive buttons do the same.
2. **Shortcuts and start.** Tick a shortcut on the desktop, an entry in the start menu
   and, if you like, a start with Windows.
3. **Install.** The runtime is copied to the chosen folder and the virtual camera is
   registered **for your Windows user** (no administrator rights needed for this). The
   progress bar fills up while it happens.

When the installation is done, two more choices appear: **Start ReCast now** and
**Remove the installer and its files**. The second one deletes the installer and
everything that is not part of the installed program, so only ReCast itself is left in
the folder you chose.

The virtual camera stays registered for your user only, and Windows asks about the
network permission the first time the phone connects. Choose **Private networks** when
it does.

Later launches reuse the installed copy, so you can pin the app to the taskbar or put
it in the Startup folder. Running it twice does not open two apps: the second copy
closes itself right away.

### The phone app

Copy `phone/ReCastPhone.apk` to the phone and open it. If Android asks, allow
"Install unknown apps" for the app you are opening it with. The APK is signed with a local
build key so that it can be sideloaded without extra tooling; it is not published on Google
Play, so Play Protect may warn about an unknown developer. Choose "Install anyway" if you
trust the file you downloaded.

The phone app requests camera and microphone access when it starts. It needs both. It
opens on a pairing screen; the camera preview and controls appear only after the PC
accepts the connection.

---

## 3. Connecting the phone

### Wi-Fi

1. Start `desktop/ReCast.exe`. It opens on the **Pair your phone** screen with a QR code.
2. On the phone, open ReCast and tap **Scan the QR code on your PC**. Point the phone at
   the code on the desktop screen.
3. The code contains the PC address (or several addresses), the port and the PIN, so the
   phone fills everything in and connects by itself. The status line turns green and
   shows the resolution, frame rate, bitrate and battery.
4. If you prefer to type it, use the fields under the QR code. The address and the
   6-digit PIN are shown there, and both have a *Copy* button on the desktop side.

Keep both devices on the same home network. Guest Wi-Fi with client isolation blocks the
connection; use USB or a phone hotspot in that case. When the PC has more than one network
adapter, the phone tries every address in the code and stops at the first one that answers.

### USB

USB is faster and does not depend on the network.

1. On the phone: *Settings > About phone > tap "Build number" seven times*.
2. *Settings > System > Developer options > enable "USB debugging"*.
3. Connect the USB cable, choose "File transfer" if Android asks, and accept "Allow USB
   debugging?" on the phone.
4. Install the Android platform-tools so that `adb.exe` is in `PATH` or in
   `%LOCALAPPDATA%\Android\Sdk\platform-tools`.
5. press **Connect over USB** in ReCast. The app installs the APK, opens the reverse tunnel
   (`adb reverse tcp:47777`) and starts the phone app with the address and PIN already
   filled in. The phone connects automatically and switches to the controls.

If you prefer, you can install the APK by hand and simply type `127.0.0.1:47777` as the
address in the phone app — the tunnel is the only thing that makes the PC reachable over
USB.

---

## 4. Using the virtual camera

Open the application you want to use (Zoom, Discord, OBS, Chrome, Edge, Teams, a browser
site…) and choose **ReCast Camera** in its camera list.

* If the camera is missing, close the other application completely and open it again so
  that it re-reads the device list.
* If the camera was selected before, pick it again once: applications remember their
  choice and do not always re-enumerate.
* 32-bit applications use the x86 filter, 64-bit applications the x64 filter. Both are
  installed.

### Using the virtual microphone

Install a virtual audio cable (for example *Voice.ai Audio Cable*), then:

1. enable **Virtual microphone** in ReCast;
2. in the other application, select the **capture** side of the cable — for example
   *Microphone (Voice.ai Audio Cable)*, not *Speakers (Voice.ai Audio Cable)*;
3. check the **Mic** meter in ReCast: it shows the level that is being sent.

ReCast detects the most common cable names automatically (Voice.ai Audio Cable, VB-CABLE,
Cable Input, DroidCam Audio, Steam Streaming Speakers). If your cable is not detected,
pick it manually under **Devices**.

### OBS Studio

* Add a **Video Capture Device** source and select *ReCast Camera*.
* Add an **Audio Input Capture** source and select the capture side of your virtual cable
  if you want the phone microphone in the recording.

### Browsers and websites

1. Start ReCast and keep **Virtual camera** enabled. The camera is usable before the
   phone connects; the picture stays a dark placeholder until a frame arrives.
2. Open the site and allow camera and microphone when it asks.
3. Choose *ReCast Camera* in the site's own camera menu. A site that still has
   an old choice such as *OBS Virtual Camera* will show nothing.
4. Quit the browser completely (every window) and open it again after a camera
   update, so it loads the new filter instead of one it already has open.

Some sites request the camera and the microphone in one step and show nothing when either
one is denied. In Windows, check *Settings > Privacy & security > Microphone* and
*Camera*, and enable "Let desktop apps access your microphone/camera".

---

## 5. The `recast` command

The first launch installs a small command line tool and adds it to your PATH, so you can
drive ReCast from any terminal without touching the window.

```console
$ recast                      # start the desktop app
$ recast status               # what is connected right now
$ recast pair                 # the address and the PIN to type into the phone
$ recast usb                  # set up the phone over USB
$ recast set resolution 1920 1080 --fps 30
$ recast set camera 0         # the id, the label or the facing direction
$ recast set mic off
$ recast set torch on
$ recast set zoom 2.5
$ recast rotate 90
$ recast theme graphite
$ recast doctor               # check the installation
$ recast uninstall            # remove the camera and the PATH entry
```

`recast --help` lists every subcommand, and `recast set --help` the settings.

The command talks to the running app over a local channel on the loopback interface that
is protected by a random token. It is not reachable from the network.

If the command is not found, open a new terminal: the PATH is updated when ReCast is
installed for the first time. `recast install` adds it again, and
`%LOCALAPPDATA%\ReCast\bin\recast.exe` always works.

---

## 6. Controls

### Connection

| Control | What it does |
| --- | --- |
| QR code | The primary way to pair. The code carries the PC addresses, the port and the one-time PIN. |
| Scan the QR code | Opens the phone's own scanner. It decodes the code offline and connects automatically. |
| Address | The PC address to type when you connect manually. |
| PIN | The 6-digit pairing code. It changes every time ReCast starts and is included in the QR code. |
| Connect over USB | Opens a tunnel through the cable and connects the phone without any network. |

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

### Microphone gain

The **Microphone gain** slider in the *Audio output* section changes the level of the
phone microphone between 0 % and 300 %. It works on the virtual microphone, so every
application that records the phone hears the change, and on the speaker monitor.

---

## 7. The phone app

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

## 8. Troubleshooting

### No video

* Check that the phone shows "Connected" and the foreground-service notification.
* The message under the connection panel explains most failures.
* Try USB, or a phone hotspot if the Wi-Fi isolates clients.
* Close other programs that feed this camera. ReCast and another sender such as
  DroidCam cannot write the same camera at the same time. Viewing apps can open it
  together.

### A website says the camera is already in use, or that it is not supported

* Quit the browser completely, including background windows, then open it again and
  pick *ReCast Camera*. A browser that still has the previous filter loaded reports
  the camera as busy or as an unsupported device.
* Sites that ask for one exact size now receive a normal capture mode (I420, YUY2,
  UYVY, RGB24 or RGB32). A combined camera-and-microphone request fails both devices
  when the camera is rejected, so the microphone can look busy as well.
* Two programs can open ReCast Camera at the same time. Two tabs inside the same
  Chrome window still share Chrome's one camera process, and that process opens a
  DirectShow camera only once.

### Discord shows the camera but the channel stays black

* Quit Discord completely, including the tray icon, then open it again.
* In Voice & Video, choose **ReCast Camera**. Older names from a previous
  install (*DirectShow Softcam*, *Rewebcam Virtual Camera*) still open the
  same camera.
* Turn the camera off and on once in the channel so Discord opens the new filter.

### A website shows no picture

* Pick *ReCast Camera* in the site's own camera selector.
* Allow both camera and microphone.
* Check *Windows Settings > Privacy & security > Microphone* and *Camera*.
* Reload the page after ReCast is connected, or reset the site's camera permission in
  the browser (padlock icon > *Site settings*) and allow it again.

### The picture is sideways or upside down

* Press **+90°** in ReCast until the picture is upright. Some phones report the wrong
  camera orientation, especially after rotating the phone.
* Press **Reset** to return to the orientation the phone reports.

### The camera is not listed in another application

* Pick *ReCast Camera* again in that application.
* Close and reopen the application so it re-reads the device list.
* 32-bit applications use the x86 filter, 64-bit applications the x64 filter; both are
  installed by the launcher.

### The microphone is silent

* Install a virtual audio cable and check that Windows shows both its playback and its
  recording endpoints.
* In the other application select the **recording** side of the cable, such as
  *CABLE Output* or *Microphone (VB-Audio Virtual Cable)*. ReCast writes the
  **playback** side, such as *CABLE Input*. Picking the recording side in ReCast
  takes the microphone the browser needs.
* Unmute *Phone microphone* and enable *Virtual microphone* in ReCast.
* Check the *Mic* meter and raise *Mic level* under **Devices** if it stays low.

### The speech is cut off or sounds watery

* Lower the RNNoise strength, or switch RNNoise off. Strong suppression removes quiet
  speech together with the noise.

### The connection fails

* The PC and the phone must be on the same network, and the address shown must be
  reachable from the phone.
* Allow `recast.exe` through Windows Firewall for private networks, or accept the
  administrator prompt on the first launch.
* The PIN must match the one on screen.

### USB setup cannot find the phone

* Install the Android platform-tools and make sure `adb.exe` is in `PATH`.
* Accept the "Allow USB debugging?" prompt on the phone.
* Use a data cable, not a charge-only cable.

### Windows warns about an unknown application

The binaries are not code-signed. Choose *More info > Run anyway* in the SmartScreen
dialog, or sign `ReCast.exe` with your own certificate.

---

## 9. Removing ReCast

1. Close ReCast.
2. Delete `%LOCALAPPDATA%\ReCast`.
3. Optional: remove the virtual camera registration and the firewall rule:

   ```powershell
   reg delete "HKCU\Software\Classes\CLSID\{8E2C0D5B-3F5A-4D7E-9B2C-6A1D4F7E9C31}" /f
   reg delete "HKCU\Software\Classes\CLSID\{AEF3B972-5FA5-4647-9571-358EB472BC9E}" /f
   reg delete "HKCU\Software\Classes\CLSID\{860BB310-5D01-11D0-BD3B-00A0C911CE86}\Instance\ReCast Camera" /f
   netsh advfirewall firewall delete rule name=ReCast
   ```

   Run the commands in an elevated PowerShell if you want the machine-wide entries removed
   as well.

4. Uninstall the phone app from the phone's app settings.

---

## 10. Privacy

The phone connects directly to the PC. Video and audio never leave your local network or
your USB link. ReCast has no accounts, no ads, no analytics and no network access other
than the local video and audio stream. The local link is not encrypted, so use it on a
network you trust.
