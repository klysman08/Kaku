import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { Copy, Download, Search, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge, Input } from "@/components/ui/form";
import { ACTION_ICONS, actionLabel } from "@/lib/actions";
import { api, type HistoryItem } from "@/lib/ipc";
import type { TabProps } from "../App";
import { Notice } from "../components";

const PAGE = 20;

function relativeTime(ts: number, locale: string) {
  const diff = ts - Date.now() / 1000;
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  const abs = Math.abs(diff);
  if (abs < 60) return rtf.format(Math.round(diff), "second");
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute");
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour");
  if (abs < 86400 * 7) return rtf.format(Math.round(diff / 86400), "day");
  return new Date(ts * 1000).toLocaleDateString(locale);
}

function TextBlock({ label, text }: { label: string; text: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  return (
    <div className="min-w-0">
      <div className="mb-1 flex items-center justify-between">
        <span className="text-muted-foreground text-xs font-medium">{label}</span>
        <Button
          variant="ghost"
          size="icon-sm"
          title={t("common.copy")}
          onClick={async () => {
            await api.copyText(text);
            setCopied(true);
            setTimeout(() => setCopied(false), 1200);
          }}
        >
          <Copy className={copied ? "text-emerald-500" : undefined} />
        </Button>
      </div>
      <p className="bg-muted/40 max-h-40 overflow-y-auto rounded-md p-2.5 text-sm whitespace-pre-wrap select-text">{text}</p>
    </div>
  );
}

export function HistoryTab({ saved }: TabProps) {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [total, setTotal] = useState(0);
  const [message, setMessage] = useState<string | null>(null);

  const load = useCallback(() => {
    void api.listHistory(query, PAGE, page * PAGE).then((r) => {
      setItems(r.items);
      setTotal(r.total);
    });
  }, [query, page]);

  useEffect(() => {
    const id = setTimeout(load, 150);
    return () => clearTimeout(id);
  }, [load]);

  useEffect(() => {
    const un = listen("usage-changed", load);
    return () => void un.then((f) => f());
  }, [load]);

  if (!saved.historyEnabled) {
    return <Notice title={t("history.disabledTitle")}>{t("history.disabledBody")}</Notice>;
  }

  async function exportCsv() {
    const path = await save({
      defaultPath: `kokorokaku-history-${new Date().toISOString().slice(0, 10)}.csv`,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    const count = await api.exportHistoryCsv(path);
    setMessage(t("history.exported", { count }));
  }

  async function clearAll() {
    if (!window.confirm(t("common.confirm"))) return;
    await api.clearHistory();
    setPage(0);
    load();
  }

  const pages = Math.max(1, Math.ceil(total / PAGE));

  return (
    <div>
      <div className="mb-4 flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="text-muted-foreground absolute top-1/2 left-3 size-4 -translate-y-1/2" />
          <Input
            className="pl-9"
            placeholder={t("history.searchPlaceholder")}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setPage(0);
            }}
          />
        </div>
        <Button variant="outline" onClick={exportCsv} disabled={total === 0}>
          <Download /> {t("common.export")}
        </Button>
        <Button variant="ghost" onClick={clearAll} disabled={total === 0}>
          <Trash2 /> {t("common.clear")}
        </Button>
      </div>

      {message && (
        <div className="mb-4">
          <Notice tone="ok">{message}</Notice>
        </div>
      )}

      {items.length === 0 ? (
        <Notice>{t("history.empty")}</Notice>
      ) : (
        <div className="space-y-3">
          {items.map((item) => {
            const Icon = ACTION_ICONS[item.action];
            return (
              <article key={item.id} className="bg-card rounded-xl border p-4">
                <header className="mb-3 flex items-center gap-2 text-sm">
                  {Icon && <Icon className="text-muted-foreground size-4" />}
                  <span className="font-medium">{actionLabel(t, item.action)}</span>
                  {item.targetLanguage && <Badge variant="secondary">{item.targetLanguage}</Badge>}
                  <span className="text-muted-foreground text-xs">· {relativeTime(item.ts, i18n.language)}</span>
                  <span className="text-muted-foreground ml-auto font-mono text-[11px]">{item.model}</span>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    title={t("common.delete")}
                    onClick={async () => {
                      await api.deleteHistory(item.id);
                      load();
                    }}
                  >
                    <Trash2 />
                  </Button>
                </header>
                <div className="grid gap-3 md:grid-cols-2">
                  <TextBlock label={t("history.input")} text={item.input} />
                  <TextBlock label={t("history.output")} text={item.output} />
                </div>
              </article>
            );
          })}
        </div>
      )}

      {total > PAGE && (
        <div className="mt-4 flex items-center justify-between text-sm">
          <span className="text-muted-foreground">{t("history.count", { count: total })}</span>
          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" disabled={page === 0} onClick={() => setPage((p) => p - 1)}>
              {t("history.prev")}
            </Button>
            <span className="text-muted-foreground tabular-nums">
              {page + 1} / {pages}
            </span>
            <Button variant="outline" size="sm" disabled={page + 1 >= pages} onClick={() => setPage((p) => p + 1)}>
              {t("history.next")}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
