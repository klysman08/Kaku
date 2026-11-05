import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { Input, Switch } from "@/components/ui/form";
import { SimpleSelect } from "@/components/ui/simple-select";
import { Button } from "@/components/ui/button";
import { api, type Settings, type SpeechEngine, type SpeechInfo } from "@/lib/ipc";
import type { TabProps } from "../App";
import { Notice, Row, Section } from "../components";

const RETENTION = [7, 30, 90, 365, 0];

/** Radix selects can't use "" as a value; "" in settings means automatic. */
const AUTO = "auto";

/** Hints for AI transcription (any language works; this only improves accuracy). */
const SPEECH_LANGUAGES = [
  { tag: "pt-BR", name: "Português (Brasil)" },
  { tag: "pt-PT", name: "Português (Portugal)" },
  { tag: "en-US", name: "English (US)" },
  { tag: "en-GB", name: "English (UK)" },
  { tag: "es-ES", name: "Español" },
  { tag: "fr-FR", name: "Français" },
  { tag: "de-DE", name: "Deutsch" },
  { tag: "it-IT", name: "Italiano" },
  { tag: "ja-JP", name: "日本語" },
  { tag: "zh-CN", name: "中文 (简体)" },
];

export function GeneralTab({ draft, update, catalog }: TabProps) {
  const { t } = useTranslation();
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [speech, setSpeech] = useState<SpeechInfo | null>(null);

  useEffect(() => {
    isEnabled().then(setAutostart, () => setAutostart(null));
    api.speechInfo().then(setSpeech, () => setSpeech(null));
  }, []);

  const native = draft.speechEngine === "native";
  const voiceTyping = draft.speechEngine === "voice_typing";
  const windowsEngines = !!speech?.native || !!speech?.voiceTyping;
  const engineDesc = !windowsEngines
    ? t("general.engineLinux")
    : native
      ? t("general.engineNativeDesc")
      : voiceTyping
        ? t("general.engineVoiceTypingDesc")
        : t("general.engineAiDesc");
  const languageOptions = [
    {
      value: AUTO,
      label:
        native && speech?.systemLanguage
          ? t("general.speechLanguageAutoNative", { lang: speech.systemLanguage })
          : t("general.speechLanguageAuto"),
    },
    ...(native
      ? (speech?.nativeLanguages ?? []).map((l) => ({ value: l.tag, label: `${l.name} (${l.tag})` }))
      : SPEECH_LANGUAGES.map((l) => ({ value: l.tag, label: l.name }))),
  ];
  // Keep a language picked for the other engine visible instead of silently blank.
  if (draft.speechLanguage && !languageOptions.some((o) => o.value === draft.speechLanguage))
    languageOptions.push({ value: draft.speechLanguage, label: draft.speechLanguage });

  async function toggleAutostart(on: boolean) {
    try {
      await (on ? enable() : disable());
      setAutostart(await isEnabled());
    } catch (e) {
      console.error(e);
    }
  }

  return (
    <div>
      <Section title={t("general.appearance")}>
        <Row label={t("general.uiLanguage")}>
          <SimpleSelect
            aria-label={t("general.uiLanguage")}
            value={draft.uiLanguage}
            onValueChange={(v) => update({ uiLanguage: v as Settings["uiLanguage"] })}
            options={[
              { value: "auto", label: t("general.auto") },
              { value: "pt-BR", label: "Português (Brasil)" },
              { value: "en", label: "English" },
            ]}
          />
        </Row>
        <Row label={t("general.theme")}>
          <SimpleSelect
            aria-label={t("general.theme")}
            value={draft.theme}
            onValueChange={(v) => update({ theme: v as Settings["theme"] })}
            options={[
              { value: "system", label: t("general.themeSystem") },
              { value: "dark", label: t("general.themeDark") },
              { value: "light", label: t("general.themeLight") },
            ]}
          />
        </Row>
        <Row label={t("general.sounds")} description={t("general.soundsDesc")}>
          <Switch checked={draft.sounds} onCheckedChange={(sounds) => update({ sounds })} />
        </Row>
      </Section>

      <Section title={t("general.window")}>
        <Row label={t("general.minimizeToTray")} description={t("general.minimizeToTrayDesc")}>
          <Switch checked={draft.minimizeToTray} onCheckedChange={(minimizeToTray) => update({ minimizeToTray })} />
        </Row>
        <Row label={t("general.autostart")} description={t("general.autostartDesc")}>
          <Switch checked={!!autostart} disabled={autostart === null} onCheckedChange={toggleAutostart} />
        </Row>
      </Section>

      <Section title={t("general.translation")}>
        <Row label={t("general.targetLanguage")} description={t("general.targetLanguageDesc")}>
          <SimpleSelect
            aria-label={t("general.targetLanguage")}
            value={draft.targetLanguage}
            onValueChange={(targetLanguage) => update({ targetLanguage })}
            options={catalog.languages.map((l) => ({ value: l, label: l }))}
          />
        </Row>
      </Section>

      <Section title={t("general.dictation")} description={t("general.dictationDesc")}>
        <Row label={t("general.speechEngine")} description={engineDesc}>
          <SimpleSelect
            aria-label={t("general.speechEngine")}
            value={draft.speechEngine}
            disabled={!windowsEngines}
            onValueChange={(v) => update({ speechEngine: v as SpeechEngine })}
            options={[
              ...(speech?.native ? [{ value: "native", label: t("general.engineNative") }] : []),
              ...(speech?.voiceTyping ? [{ value: "voice_typing", label: t("general.engineVoiceTyping") }] : []),
              { value: "ai", label: t("general.engineAi") },
            ]}
          />
        </Row>
        <Row label={t("general.speechLanguage")} description={voiceTyping ? t("general.speechLanguageVoiceTyping") : undefined}>
          <SimpleSelect
            disabled={voiceTyping}
            aria-label={t("general.speechLanguage")}
            value={draft.speechLanguage || AUTO}
            onValueChange={(v) => update({ speechLanguage: v === AUTO ? "" : v })}
            options={languageOptions}
          />
        </Row>
        {!native && draft.provider === "openai" && (
          <Row label={t("general.transcriptionModel")} description={t("general.transcriptionModelDesc")} stack>
            <Input
              value={draft.openaiTranscriptionModel}
              spellCheck={false}
              placeholder="gpt-4o-mini-transcribe"
              onChange={(e) => update({ openaiTranscriptionModel: e.target.value })}
            />
          </Row>
        )}
        {(native || voiceTyping) && (
          <div className="px-4 py-3">
            <Notice>
              {t("errors.speechPrivacy")}
              <div className="mt-2">
                <Button variant="outline" size="sm" onClick={() => void api.openSpeechSettings()}>
                  {t("general.openSpeechSettings")}
                </Button>
              </div>
            </Notice>
          </div>
        )}
      </Section>

      <Section title={t("general.privacy")}>
        <Row label={t("general.history")} description={t("general.historyDesc")}>
          <Switch checked={draft.historyEnabled} onCheckedChange={(historyEnabled) => update({ historyEnabled })} />
        </Row>
        <Row label={t("general.retention")}>
          <SimpleSelect
            aria-label={t("general.retention")}
            disabled={!draft.historyEnabled}
            value={String(draft.historyRetentionDays)}
            onValueChange={(v) => update({ historyRetentionDays: Number(v) })}
            options={RETENTION.map((d) => ({ value: String(d), label: d === 0 ? t("general.forever") : t("general.days", { count: d }) }))}
          />
        </Row>
      </Section>

      <Section title={t("general.network")}>
        <Row label={t("general.proxy")} description={t("general.proxyDesc")} stack>
          <Input
            value={draft.proxyUrl}
            spellCheck={false}
            placeholder="http://proxy:8080"
            onChange={(e) => update({ proxyUrl: e.target.value })}
          />
        </Row>
        <Row label={t("general.timeout")}>
          <Input
            type="number"
            min={5}
            max={300}
            className="w-28"
            value={draft.requestTimeoutSecs}
            onChange={(e) => update({ requestTimeoutSecs: Number(e.target.value) || 45 })}
          />
        </Row>
      </Section>

      <Section title={t("general.updates")}>
        <Row label={t("general.autoUpdate")} description={t("general.autoUpdateDesc")}>
          <Switch checked={draft.autoUpdate} onCheckedChange={(autoUpdate) => update({ autoUpdate })} />
        </Row>
      </Section>
    </div>
  );
}
