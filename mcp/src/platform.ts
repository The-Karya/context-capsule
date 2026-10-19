// context-capsule — mcp: words that depend on the OS (hotkeys, how to open the app, the picker)
export type OS = "mac" | "windows" | "linux";

export function currentOS(platform: string = process.platform): OS {
  if (platform === "win32") return "windows";
  if (platform === "darwin") return "mac";
  return "linux";
}

export interface PlatformWords {
  /** Start/stop recording, e.g. "⌥C (Option-C)". */
  recordKey: string;
  /** Short form, e.g. "⌥C". */
  recordKeyShort: string;
  voiceKeyShort: string;
  /** "the macOS picker" / "the Windows picker" / "the screen-sharing picker". */
  picker: string;
  /** Where the app's icon lives. */
  tray: string;
  /** How to open the recorder app. */
  openApp: string;
  /** "this Mac" / "this PC" / "this computer". */
  machine: string;
}

const WORDS: Record<OS, PlatformWords> = {
  mac: {
    recordKey: "⌥C (Option-C)",
    recordKeyShort: "⌥C",
    voiceKeyShort: "⌥V",
    picker: "the macOS picker",
    tray: "menu bar",
    openApp:
      "run `open -a \"Context Capsule\"` in Terminal, or double-click Context Capsule.app " +
      "(after building it with `capture/build.sh` it is in `capture/build/Context Capsule.app`; " +
      "you can drag it to /Applications). A circle icon appears in the menu bar when it's ready.",
    machine: "this Mac",
  },
  windows: {
    recordKey: "Ctrl+Alt+Shift+C",
    recordKeyShort: "Ctrl+Alt+Shift+C",
    voiceKeyShort: "Ctrl+Alt+Shift+V",
    picker: "the Windows picker",
    tray: "system tray (bottom right; it may be under the ^ arrow)",
    openApp:
      "start Context Capsule from the Start menu, or run `context-capsule.exe` " +
      "(after building it with `capture-rs/build.ps1` it is in `capture-rs/target/release/`). " +
      "A circle icon appears in the system tray when it's ready.",
    machine: "this PC",
  },
  linux: {
    recordKey: "Ctrl+Alt+Shift+C",
    recordKeyShort: "Ctrl+Alt+Shift+C",
    voiceKeyShort: "Ctrl+Alt+Shift+V",
    picker: "the screen-sharing picker",
    tray: "panel's tray",
    openApp:
      "run `context-capsule &` in a terminal " +
      "(after building it with `capture-rs/build.sh` it is in `capture-rs/target/release/`). " +
      "A circle icon appears in the tray when it's ready.",
    machine: "this computer",
  },
};

export function words(os: OS = currentOS()): PlatformWords {
  return WORDS[os];
}
