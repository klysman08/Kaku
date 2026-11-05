import {
  Briefcase,
  FileText,
  Hash,
  Heart,
  List,
  PenLine,
  Scissors,
  Smile,
  SquareCheck,
  WandSparkles,
  Languages,
  Mic,
  type LucideIcon,
} from "lucide-react";
import type { TFunction } from "i18next";
import { isCommandError } from "./ipc";

export const ACTION_ICONS: Record<string, LucideIcon> = {
  rewrite: PenLine,
  review: SquareCheck,
  professional: Briefcase,
  appealing: WandSparkles,
  friendly: Heart,
  simplify: Hash,
  summarize: List,
  detail: FileText,
  emojis: Smile,
  shorten: Scissors,
  translate: Languages,
  dictation: Mic,
};

export function actionLabel(t: TFunction, id: string): string {
  return t(`actions.${id}`, { defaultValue: id });
}

const SPEECH_ERRORS: Record<string, string> = {
  speech_privacy: "speechPrivacy",
  speech_mic: "speechMic",
  speech_language: "speechLanguage",
  speech_empty: "speechEmpty",
  speech_audio: "speechAudio",
};

/** Map a command error to a friendly, translated message (mirrors TextBetter's renderError). */
export function describeError(t: TFunction, e: unknown): { title: string; detail: string } {
  if (!isCommandError(e)) return { title: t("errors.generic"), detail: String(e) };
  const status = Number(e.code);
  if (e.code === "missing_key") return { title: t("errors.setupTitle"), detail: t("errors.missingKey") };
  if (status === 400 || status === 401 || status === 403)
    return { title: t("errors.configTitle"), detail: `${t("errors.config")}\n\n${e.message}` };
  if (status === 404) return { title: t("errors.modelTitle"), detail: `${t("errors.model")}\n\n${e.message}` };
  if (status === 429) return { title: t("errors.rateTitle"), detail: `${t("errors.rate")}\n\n${e.message}` };
  if (status >= 500) return { title: t("errors.serverTitle", { status }), detail: `${t("errors.server")}\n\n${e.message}` };
  if (e.code === "timeout") return { title: t("errors.timeoutTitle"), detail: t("errors.timeout") };
  if (e.code === "network") return { title: t("errors.networkTitle"), detail: `${t("errors.network")}\n\n${e.message}` };
  const speech = SPEECH_ERRORS[e.code];
  if (speech) return { title: t(`errors.${speech}Title`), detail: t(`errors.${speech}`) };
  if (e.code === "empty") return { title: t("errors.emptyTitle"), detail: t("errors.empty") };
  return { title: t("errors.generic"), detail: e.message };
}
