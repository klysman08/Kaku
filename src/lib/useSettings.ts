import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Settings } from "./ipc";
import { applyTheme } from "./theme";
import { applyLanguage } from "@/i18n";

/** Live settings for read-only windows (popup/toast): follows "settings-changed" events. */
export function useLiveSettings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  useEffect(() => {
    const apply = (s: Settings) => {
      setSettings(s);
      applyTheme(s.theme);
      applyLanguage(s.uiLanguage);
    };
    void api.getSettings().then(apply);
    const un = listen<Settings>("settings-changed", (e) => apply(e.payload));
    return () => void un.then((f) => f());
  }, []);
  return settings;
}
