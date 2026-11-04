import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { BarChart3, Info, Keyboard, KeyRound, ListChecks, History, Settings2 } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { Highlight } from "@animate-ui/react/primitives/effects/highlight";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { api, type Catalog, type Settings, type ShortcutFailure } from "@/lib/ipc";
import { applyTheme } from "@/lib/theme";
import { applyLanguage } from "@/i18n";
import { GeneralTab } from "./tabs/GeneralTab";
import { ProviderTab } from "./tabs/ProviderTab";
import { ShortcutsTab } from "./tabs/ShortcutsTab";
import { ActionsTab } from "./tabs/ActionsTab";
import { DashboardTab } from "./tabs/DashboardTab";
import { HistoryTab } from "./tabs/HistoryTab";
import { AboutTab } from "./tabs/AboutTab";
import { TitleBar } from "./TitleBar";

export interface TabProps {
  draft: Settings;
  saved: Settings;
  update: (patch: Partial<Settings>) => void;
  /** Apply a patch and persist immediately (with any other pending edits). */
  commit: (patch: Partial<Settings>) => Promise<void>;
  catalog: Catalog;
  failures: ShortcutFailure[];
  setFailures: (f: ShortcutFailure[]) => void;
}

const TABS = [
  { id: "provider", icon: KeyRound },
  { id: "shortcuts", icon: Keyboard },
  { id: "actions", icon: ListChecks },
  { id: "general", icon: Settings2 },
  { id: "dashboard", icon: BarChart3 },
  { id: "history", icon: History },
  { id: "about", icon: Info },
] as const;

export function App() {
  const { t } = useTranslation();
  const [saved, setSaved] = useState<Settings | null>(null);
  const [draft, setDraft] = useState<Settings | null>(null);
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [failures, setFailures] = useState<ShortcutFailure[]>([]);
  const [tab, setTab] = useState<string>("provider");
  const [saving, setSaving] = useState(false);
  const [justSaved, setJustSaved] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  useEffect(() => scrollRef.current?.scrollTo({ top: 0 }), [tab]);
  const savedRef = useRef<Settings | null>(null);
  const draftRef = useRef<Settings | null>(null);
  savedRef.current = saved;
  draftRef.current = draft;

  const adopt = useCallback((s: Settings) => {
    setSaved(s);
    setDraft(s);
    applyTheme(s.theme);
    applyLanguage(s.uiLanguage);
  }, []);

  useEffect(() => {
    void Promise.all([api.getSettings(), api.getCatalog(), api.shortcutStatus()]).then(([s, c, f]) => {
      adopt(s);
      setCatalog(c);
      setFailures(f);
      if (s.onboarded) setTab("dashboard");
    });
    // Changes made elsewhere (tray "pause") — adopt only when nothing is being edited.
    const unSettings = listen<Settings>("settings-changed", (e) => {
      const clean = JSON.stringify(draftRef.current) === JSON.stringify(savedRef.current);
      setSaved(e.payload);
      if (clean) setDraft(e.payload);
    });
    const unNav = listen<string>("navigate", (e) => setTab(e.payload));
    return () => {
      void unSettings.then((f) => f());
      void unNav.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const dirty = useMemo(() => !!draft && !!saved && JSON.stringify(draft) !== JSON.stringify(saved), [draft, saved]);

  // Live-preview theme and language while editing.
  useEffect(() => {
    if (!draft) return;
    applyTheme(draft.theme);
    applyLanguage(draft.uiLanguage);
  }, [draft?.theme, draft?.uiLanguage]); // eslint-disable-line react-hooks/exhaustive-deps

  const update = useCallback((patch: Partial<Settings>) => setDraft((d) => (d ? { ...d, ...patch } : d)), []);

  const commit = useCallback(async (patch: Partial<Settings>) => {
    const next = { ...(draftRef.current as Settings), ...patch };
    setDraft(next);
    setFailures(await api.saveSettings(next));
    setSaved(next);
  }, []);

  async function save() {
    if (!draft) return;
    setSaving(true);
    try {
      const f = await api.saveSettings(draft);
      setFailures(f);
      setSaved(draft);
      setJustSaved(true);
      setTimeout(() => setJustSaved(false), 1800);
    } finally {
      setSaving(false);
    }
  }

  if (!draft || !saved || !catalog) {
    return <div className="text-muted-foreground grid h-full place-items-center text-sm">{t("common.loading")}</div>;
  }

  const props: TabProps = { draft, saved, update, commit, catalog, failures, setFailures };

  return (
    <Tabs value={tab} onValueChange={setTab} orientation="vertical" className="h-full gap-0">
      <aside className="bg-muted/30 flex w-56 shrink-0 flex-col border-r px-3 pb-3">
        <div data-tauri-drag-region className="flex items-center gap-2.5 px-2 pt-4 pb-6">
          <img src="/app-icon.svg" alt="" draggable={false} className="pointer-events-none size-8 rounded-lg shadow-sm" />
          <div className="pointer-events-none">
            <div className="text-sm font-semibold">KokoroKaku</div>
            <div className="text-muted-foreground text-[11px] leading-tight">{t("app.tagline")}</div>
          </div>
        </div>
        <TabsList className="flex-col items-stretch gap-0.5">
          <Highlight
            value={tab}
            click={false}
            className="bg-accent rounded-md"
            transition={{ type: "spring", stiffness: 380, damping: 32 }}
          >
            {TABS.map(({ id, icon: Icon }) => (
              <TabsTrigger key={id} value={id} data-value={id} className="justify-start data-[state=active]:bg-transparent">
                <Icon />
                {t(`nav.${id}`)}
              </TabsTrigger>
            ))}
          </Highlight>
        </TabsList>
      </aside>

      <main className="relative flex min-w-0 flex-1 flex-col">
        <TitleBar title="" minimizeToTray={saved.minimizeToTray} />
        <div ref={scrollRef} className="flex-1 overflow-y-auto">
          <div className="mx-auto max-w-3xl px-8 pt-1 pb-24">
            <h1 className="mb-6 text-xl font-semibold tracking-tight">{t(`nav.${tab}`)}</h1>
            <TabsContent value="provider">
              <ProviderTab {...props} />
            </TabsContent>
            <TabsContent value="shortcuts">
              <ShortcutsTab {...props} />
            </TabsContent>
            <TabsContent value="actions">
              <ActionsTab {...props} />
            </TabsContent>
            <TabsContent value="general">
              <GeneralTab {...props} />
            </TabsContent>
            <TabsContent value="dashboard">
              <DashboardTab {...props} />
            </TabsContent>
            <TabsContent value="history">
              <HistoryTab {...props} />
            </TabsContent>
            <TabsContent value="about">
              <AboutTab {...props} />
            </TabsContent>
          </div>
        </div>

        <AnimatePresence>
          {(dirty || justSaved) && (
            <motion.div
              initial={{ y: 24, opacity: 0 }}
              animate={{ y: 0, opacity: 1 }}
              exit={{ y: 24, opacity: 0 }}
              transition={{ type: "spring", stiffness: 420, damping: 34 }}
              className="bg-background/95 absolute inset-x-0 bottom-0 flex items-center justify-end gap-2 border-t px-8 py-3 backdrop-blur"
            >
              <span className="text-muted-foreground mr-auto text-sm">{dirty ? t("common.unsaved") : t("common.saved")}</span>
              {dirty && (
                <>
                  <Button variant="ghost" onClick={() => setDraft(saved)}>
                    {t("common.discard")}
                  </Button>
                  <Button onClick={save} disabled={saving}>
                    {t("common.save")}
                  </Button>
                </>
              )}
            </motion.div>
          )}
        </AnimatePresence>
      </main>
    </Tabs>
  );
}
