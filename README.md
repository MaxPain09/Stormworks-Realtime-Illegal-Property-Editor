# SWRIPE

Stormworks Realtime Illegal Property Editor

A thing I made because I got tired of editing XML.

SWRIPE lets you change component properties beyond the limits normally allowed by Stormworks without editing vehicle XML files.

Enable Capture Mode, select a component with the normal Stormworks select tool, and SWRIPE will attempt to capture the properties being accessed by Stormworks.

Once captured, the values can be viewed and modified directly from the overlay.

Changes can be applied instantly or manually.

If you need to inspect or continue editing an already modified component, enable Prevent Stormworks Property Writes to stop Stormworks from clamping values back to legal ranges while the component is selected.

## How To Use

1. Start Stormworks.
2. Run the injector.
3. Enable Capture Mode (F2).
4. Select a component using the normal Stormworks select tool.
5. Click one of the component's properties.
6. SWRIPE will attempt to capture the property addresses and read their current values.
7. Edit values using:
   - The number input boxes
   - The +/- buttons
   - Left/Right arrow keys to select a property
   - Up/Down arrow keys to change the selected value
8. Enable Instant Update to apply changes immediately, or click Apply to Selected Component.

If things start behaving strangely, try spamming Clear Selection and selecting the component again.

## Prevent Stormworks Property Writes

This option prevents Stormworks from writing property values back to the selected component.

This is mainly useful when working with illegal values.

Normally, selecting a modified component may cause Stormworks to clamp values back into the normal editor limits.

When Prevent Stormworks Property Writes is enabled:

- Illegal values remain unchanged
- Existing illegal values can be inspected
- Existing illegal values can be edited further
- Stormworks property sliders stop working while the option is enabled

## Technical Details

SWRIPE does not use fixed addresses, XML parsing, or Cheat Engine scans.

Instead, it watches a Stormworks property write instruction and captures the memory addresses being used by the game when a component property is selected.

The captured addresses are then used to read and write values directly from memory.

Because of how the capture system works, SWRIPE currently only works with float based properties.

A good rule of thumb is:

> If the property has a slider in Stormworks, SWRIPE will probably be able to capture it.

Things I've tested so far:

- Wheels
- Suspension wheels
- Pivots
- Rotors
- Solid rockets

Probably other stuff too.

## Antivirus Warnings

SWRIPE uses DLL injection and memory modification.

The injector uses APIs such as:

- OpenProcess
- VirtualAllocEx
- WriteProcessMemory
- CreateRemoteThread

These are commonly used by debuggers, trainers, and unfortunately malware, so some antivirus products may complain.

Windows Defender actually quarantined the injector during development, which was a fun way to discover I'd accidentally made something that behaves exactly like malware from an antivirus point of view. 😅

If you have concerns, the source code is available to inspect and build from. 🙂

## Disclaimer

Not affiliated with Stormworks or Geometa.
