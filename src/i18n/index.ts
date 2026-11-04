import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./en.json";
import ptBR from "./pt-BR.json";
import type { Settings } from "@/lib/ipc";

export function resolveLanguage(setting: Settings["uiLanguage"] | undefined): "pt-BR" | "en" {
  if (setting === "pt-BR" || setting === "en") return setting;
  return navigator.language.toLowerCase().startsWith("pt") ? "pt-BR" : "en";
}

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, "pt-BR": { translation: ptBR } },
  lng: resolveLanguage("auto"),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
  returnNull: false,
});

export function applyLanguage(setting: Settings["uiLanguage"]) {
  const lng = resolveLanguage(setting);
  if (i18n.language !== lng) void i18n.changeLanguage(lng);
  document.documentElement.lang = lng;
}

export default i18n;
