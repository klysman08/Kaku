import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Pause, Play } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ACTION_ICONS, actionLabel } from "@/lib/actions";
import { api, type AppInfo } from "@/lib/ipc";
import { findDuplicates } from "@/lib/shortcut";
import type { TabProps } from "../App";
import { Notice, Row, Section } from "../components";
import { ShortcutRecorder } from "../ShortcutRecorder";

export function ShortcutsTab({ draft, saved, update, commit, catalog, failures }: TabProps) {
  const { t } = useTranslation();
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(() => void api.appInfo().then(setInfo), []);

  const combos = useMemo(() => ({ popup: draft.shortcuts.popup, ...draft.shortcuts.actions }), [draft.shortcuts]);
  const duplicates = useMemo(() => findDuplicates(combos), [combos]);
  // Registration failures only apply to what is saved right now.
  const failureFor = (name: string) => {
    if (duplicates.has(name)) return t("shortcuts.duplicate");
    const f = failures.find((x) => x.command === name);
    const current = name === "popup" ? saved.shortcuts.popup : saved.shortcuts.actions[name];
    if (f && f.shortcut === current && current === combos[name as keyof typeof combos]) return t(`shortcuts.${f.reason}`);
    return undefined;
  };

  const setAction = (id: string, value: string) => {
    const actions = { ...draft.shortcuts.actions };
    if (value) actions[id] = value;
    else delete actions[id];
    update({ shortcuts: { ...draft.shortcuts, actions } });
  };

  const wayland = info?.sessionType === "wayland";

  return (
    <div>
      {saved.paused && (
        <div className="mb-6">
          <Notice tone="warn" title={t("shortcuts.paused")}>
            {t("shortcuts.pausedDesc")}
            <div className="mt-3">
              <Button size="sm" onClick={() => void commit({ paused: false })}>
                <Play /> {t("shortcuts.resume")}
              </Button>
            </div>
          </Notice>
        </div>
      )}

      {wayland && (
        <div className="mb-6">
          <Notice tone="warn" title={t("shortcuts.waylandTitle")}>
            {t("shortcuts.waylandBody")}
            <pre className="bg-muted mt-2 rounded-md p-2 font-mono text-xs">kokorokaku --action review --clipboard{"\n"}kokorokaku --popup</pre>
          </Notice>
        </div>
      )}

      <Section title={t("shortcuts.popupTitle")} description={t("shortcuts.popupDesc")}>
        <Row label={t("shortcuts.popupTitle")}>
          <ShortcutRecorder
            value={draft.shortcuts.popup}
            error={failureFor("popup")}
            onChange={(v) => update({ shortcuts: { ...draft.shortcuts, popup: v } })}
          />
        </Row>
      </Section>

      <Section title={t("shortcuts.directTitle")} description={t("shortcuts.directDesc")}>
        {catalog.actions.map((a) => {
          const Icon = ACTION_ICONS[a.id];
          return (
            <Row
              key={a.id}
              label={
                <span className="inline-flex items-center gap-2">
                  {Icon && <Icon className="text-muted-foreground size-4" />}
                  {actionLabel(t, a.id)}
                  {a.id === "translate" && <span className="text-muted-foreground text-xs font-normal">→ {draft.targetLanguage}</span>}
                </span>
              }
            >
              <ShortcutRecorder value={draft.shortcuts.actions[a.id] ?? ""} error={failureFor(a.id)} onChange={(v) => setAction(a.id, v)} />
            </Row>
          );
        })}
      </Section>

      <Section title={t("shortcuts.tipsTitle")}>
        <Row label={t("shortcuts.pause")} description={t("shortcuts.pausedDesc")}>
          <Button variant="outline" size="sm" onClick={() => void commit({ paused: !saved.paused })}>
            {saved.paused ? <Play /> : <Pause />}
            {saved.paused ? t("shortcuts.resume") : t("shortcuts.pause")}
          </Button>
        </Row>
        <Row label="Ctrl+Alt" description={t("shortcuts.tipAltGr")} />
        <Row label={t("shortcuts.clipboardTitle")} description={t("shortcuts.tipClipboard")} />
        <Row
          label={t("shortcuts.cliTitle")}
          description={<code className="font-mono">kokorokaku --action &lt;id&gt; [--clipboard] · --popup · --settings</code>}
        />
      </Section>
    </div>
  );
}
