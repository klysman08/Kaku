import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, ExternalLink, FolderGit2, Loader2, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { api, type AppInfo } from "@/lib/ipc";
import type { TabProps } from "../App";
import { Notice, Row, Section } from "../components";

const REPO = "https://github.com/klysman08/KokoroKaku";

export function AboutTab(_: TabProps) {
  const { t } = useTranslation();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [state, setState] = useState<
    | { kind: "idle" }
    | { kind: "checking" }
    | { kind: "none" }
    | { kind: "available"; update: Update }
    | { kind: "downloading"; percent: number }
    | { kind: "error"; message: string }
  >({ kind: "idle" });

  useEffect(() => void api.appInfo().then(setInfo), []);

  async function checkNow() {
    setState({ kind: "checking" });
    try {
      const update = await check();
      setState(update ? { kind: "available", update } : { kind: "none" });
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }

  async function install(update: Update) {
    let total = 0;
    let done = 0;
    setState({ kind: "downloading", percent: 0 });
    try {
      await update.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        if (ev.event === "Progress") {
          done += ev.data.chunkLength;
          setState({ kind: "downloading", percent: total ? Math.round((done / total) * 100) : 0 });
        }
      });
      await relaunch();
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }

  return (
    <div>
      <div className="mb-8 flex items-center gap-4">
        <img src="/app-icon.png" alt="" className="size-14 rounded-2xl" />
        <div>
          <div className="text-lg font-semibold">KokoroKaku</div>
          <div className="text-muted-foreground text-sm">{info && t("about.version", { version: info.version })}</div>
        </div>
      </div>

      <Section title={t("general.updates")}>
        <div className="space-y-3 p-4">
          <div className="flex items-center gap-2">
            <Button variant="outline" onClick={checkNow} disabled={state.kind === "checking" || state.kind === "downloading"}>
              {state.kind === "checking" ? <Loader2 className="animate-spin" /> : <RefreshCw />}
              {state.kind === "checking" ? t("about.checking") : t("about.check")}
            </Button>
            {state.kind === "available" && (
              <Button onClick={() => install(state.update)}>
                <Download /> {t("about.install")}
              </Button>
            )}
          </div>
          {state.kind === "none" && <Notice tone="ok">{t("about.upToDate")}</Notice>}
          {state.kind === "available" && (
            <Notice title={t("about.available", { version: state.update.version })}>{state.update.body}</Notice>
          )}
          {state.kind === "downloading" && <Notice>{t("about.installing", { percent: state.percent })}</Notice>}
          {state.kind === "error" && <Notice tone="error">{t("about.updateError", { message: state.message })}</Notice>}
        </div>
      </Section>

      <Section title={t("about.privacyTitle")}>
        <Row label={t("about.privacyTitle")} description={t("about.privacyBody")} />
        <Row label="MIT" description={t("about.license")}>
          <Button variant="outline" size="sm" onClick={() => void openUrl(REPO)}>
            <FolderGit2 /> {t("about.repo")} <ExternalLink className="size-3" />
          </Button>
        </Row>
        {info && (
          <>
            <Row label={t("about.data")} description={<code className="font-mono select-text">{info.dataDir}</code>} />
            <Row label={t("about.logs")} description={<code className="font-mono select-text">{info.logDir}</code>} />
          </>
        )}
      </Section>
    </div>
  );
}
