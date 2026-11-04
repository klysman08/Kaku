import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { Input, Switch } from "@/components/ui/form";
import { SimpleSelect } from "@/components/ui/simple-select";
import type { Settings } from "@/lib/ipc";
import type { TabProps } from "../App";
import { Row, Section } from "../components";

const RETENTION = [7, 30, 90, 365, 0];

export function GeneralTab({ draft, update, catalog }: TabProps) {
  const { t } = useTranslation();
  const [autostart, setAutostart] = useState<boolean | null>(null);

  useEffect(() => {
    isEnabled().then(setAutostart, () => setAutostart(null));
  }, []);

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
