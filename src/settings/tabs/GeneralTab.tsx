import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { Input, NativeSelect, Switch } from "@/components/ui/form";
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
          <NativeSelect className="w-52" value={draft.uiLanguage} onChange={(e) => update({ uiLanguage: e.target.value as Settings["uiLanguage"] })}>
            <option value="auto">{t("general.auto")}</option>
            <option value="pt-BR">Português (Brasil)</option>
            <option value="en">English</option>
          </NativeSelect>
        </Row>
        <Row label={t("general.theme")}>
          <NativeSelect className="w-52" value={draft.theme} onChange={(e) => update({ theme: e.target.value as Settings["theme"] })}>
            <option value="system">{t("general.themeSystem")}</option>
            <option value="dark">{t("general.themeDark")}</option>
            <option value="light">{t("general.themeLight")}</option>
          </NativeSelect>
        </Row>
        <Row label={t("general.sounds")} description={t("general.soundsDesc")}>
          <Switch checked={draft.sounds} onCheckedChange={(sounds) => update({ sounds })} />
        </Row>
      </Section>

      <Section title={t("general.startup")}>
        <Row label={t("general.autostart")} description={t("general.autostartDesc")}>
          <Switch checked={!!autostart} disabled={autostart === null} onCheckedChange={toggleAutostart} />
        </Row>
      </Section>

      <Section title={t("general.translation")}>
        <Row label={t("general.targetLanguage")} description={t("general.targetLanguageDesc")}>
          <NativeSelect className="w-52" value={draft.targetLanguage} onChange={(e) => update({ targetLanguage: e.target.value })}>
            {catalog.languages.map((l) => (
              <option key={l} value={l}>
                {l}
              </option>
            ))}
          </NativeSelect>
        </Row>
      </Section>

      <Section title={t("general.privacy")}>
        <Row label={t("general.history")} description={t("general.historyDesc")}>
          <Switch checked={draft.historyEnabled} onCheckedChange={(historyEnabled) => update({ historyEnabled })} />
        </Row>
        <Row label={t("general.retention")}>
          <NativeSelect
            className="w-52"
            disabled={!draft.historyEnabled}
            value={draft.historyRetentionDays}
            onChange={(e) => update({ historyRetentionDays: Number(e.target.value) })}
          >
            {RETENTION.map((d) => (
              <option key={d} value={d}>
                {d === 0 ? t("general.forever") : t("general.days", { count: d })}
              </option>
            ))}
          </NativeSelect>
        </Row>
      </Section>

      <Section title={t("general.network")}>
        <Row label={t("general.proxy")} description={t("general.proxyDesc")} stack>
          <Input value={draft.proxyUrl} spellCheck={false} placeholder="http://proxy:8080" onChange={(e) => update({ proxyUrl: e.target.value })} />
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
