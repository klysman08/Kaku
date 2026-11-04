// Shortcut recorder helpers. Output format ("Ctrl+Shift+K") is parsed by
// tauri-plugin-global-shortcut on the Rust side.

const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "ShiftLeft",
  "ShiftRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

/** Map KeyboardEvent.code to the key token the global-shortcut parser accepts. */
export function keyToken(code: string): string | null {
  if (MODIFIER_CODES.has(code)) return null;
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  if (/^Numpad[0-9]$/.test(code)) return code;
  const named: Record<string, string> = {
    Space: "Space",
    Enter: "Enter",
    Tab: "Tab",
    Backspace: "Backspace",
    Delete: "Delete",
    Insert: "Insert",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    ArrowUp: "ArrowUp",
    ArrowDown: "ArrowDown",
    ArrowLeft: "ArrowLeft",
    ArrowRight: "ArrowRight",
    Minus: "Minus",
    Equal: "Equal",
    BracketLeft: "BracketLeft",
    BracketRight: "BracketRight",
    Backslash: "Backslash",
    Semicolon: "Semicolon",
    Quote: "Quote",
    Comma: "Comma",
    Period: "Period",
    Slash: "Slash",
    Backquote: "Backquote",
  };
  return named[code] ?? null;
}

export type RecordResult =
  | { kind: "combo"; value: string }
  | { kind: "clear" }
  | { kind: "cancel" }
  | { kind: "pending" }
  | { kind: "needsModifier" };

interface KeyLike {
  code: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/**
 * Turn a keydown into a shortcut string. Global shortcuts need at least Ctrl, Alt or Super
 * (Shift alone would swallow normal typing). Esc cancels, Backspace/Delete alone clears.
 */
export function recordKey(e: KeyLike): RecordResult {
  const hasMod = e.ctrlKey || e.altKey || e.metaKey;
  if (!hasMod && !e.shiftKey) {
    if (e.code === "Escape") return { kind: "cancel" };
    if (e.code === "Backspace" || e.code === "Delete") return { kind: "clear" };
  }
  const key = keyToken(e.code);
  if (!key) return { kind: "pending" };
  if (!hasMod && !/^F\d+$/.test(key)) return { kind: "needsModifier" };
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Super");
  parts.push(key);
  return { kind: "combo", value: parts.join("+") };
}

/** Human display: "Ctrl+Shift+Space" → ["Ctrl", "Shift", "Space"]. */
export function displayParts(combo: string, isMac = false): string[] {
  if (!combo) return [];
  return combo.split("+").map((p) => (p === "Super" ? (isMac ? "⌘" : "Win") : p));
}

/** Normalised identity used to detect duplicates in the UI (order-insensitive modifiers). */
export function normalize(combo: string): string {
  const parts = combo
    .split("+")
    .map((p) => p.trim().toLowerCase())
    .filter(Boolean)
    .map((p) => (p === "control" ? "ctrl" : p === "cmd" || p === "meta" ? "super" : p));
  const key = parts.pop() ?? "";
  return [...parts.sort(), key].join("+");
}

export function findDuplicates(combos: Record<string, string>): Set<string> {
  const seen = new Map<string, string>();
  const dupes = new Set<string>();
  for (const [name, combo] of Object.entries(combos)) {
    if (!combo) continue;
    const n = normalize(combo);
    const prev = seen.get(n);
    if (prev) {
      dupes.add(prev);
      dupes.add(name);
    } else seen.set(n, name);
  }
  return dupes;
}
