# SWRIPE

Stormworks Realtime Illegal Property Editor

A thing I made because I got tired of editing XML.

SWRIPE runs inside Stormworks and lets you edit component properties directly while the game is running.

It was originally made for wheels, but it turns out the same capture system works on a lot of other components too.

If a property is stored as a float, SWRIPE can often find it and let you change it.

Things I've tested so far:

- Wheels
- Suspension wheels
- Pivots
- Rotors
- Solid rockets

Probably other stuff too.

## Warning

SWRIPE uses DLL injection and memory editing.

Windows Defender and other antivirus software may complain about it.

Windows Defender actually quarantined the injector during development, which was a fun way to discover I'd accidentally made something that behaves exactly like malware from an antivirus point of view. 😅

If you have concerns, the source code is available to inspect and build from. 🙂

## Disclaimer

Not affiliated with Stormworks or Geometa.
