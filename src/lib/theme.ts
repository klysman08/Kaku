import type { Settings } from "./ipc";

const media = window.matchMedia("(prefers-color-scheme: dark)");
let current: Settings["theme"] = "system";

function paint() {
  const dark = current === "dark" || (current === "system" && media.matches);
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
}

media.addEventListener("change", paint);

export function applyTheme(theme: Settings["theme"]) {
  current = theme;
  paint();
}
