/** Converts a keydown into a Tauri global-shortcut accelerator ("Alt+Z"). */
export function acceleratorFromEvent(e: KeyboardEvent, isMac: boolean): string | null {
  const key = keyName(e.code);
  if (!key) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Control");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push(isMac ? "Command" : "Super");
  const isFunctionKey = /^F\d+$/.test(key);
  // Shift alone would hijack normal capital letters.
  const hasRealModifier = mods.some((m) => m !== "Shift");
  if (!isFunctionKey && !hasRealModifier) return null;
  return [...mods, key].join("+");
}

function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-2])$/.test(code)) return code;
  if (code === "Space") return "Space";
  return null;
}

const MAC_SYMBOLS: Record<string, string> = {
  Control: "⌃",
  Alt: "⌥",
  Shift: "⇧",
  Command: "⌘",
};

export function displayShortcut(accel: string, isMac: boolean): string {
  const parts = accel.split("+");
  if (isMac) return parts.map((p) => MAC_SYMBOLS[p] ?? p).join("");
  return parts.map((p) => (p === "Control" ? "Ctrl" : p === "Super" ? "Win" : p)).join(" + ");
}
