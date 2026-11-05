import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { ArrowLeft, Check, ClipboardCopy, Globe, Loader2, Replace, RotateCcw, Settings, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { Highlight } from "@animate-ui/react/primitives/effects/highlight";
import { ACTION_ICONS, actionLabel, describeError } from "@/lib/actions";
import { api, type Catalog, type PopupPayload } from "@/lib/ipc";
import { playSound } from "@/lib/sound";
import { useLiveSettings } from "@/lib/useSettings";
import { cn } from "@/lib/utils";

type View =
  | { kind: "menu" }
  | { kind: "working"; action: string }
  | { kind: "result"; action: string; text: string }
  | { kind: "error"; action: string; title: string; detail: string };

const WIDTH = 340;

export function Popup() {
  const { t } = useTranslation();
  const settings = useLiveSettings();
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [payload, setPayload] = useState<PopupPayload>({ text: "", hasSelection: false, canReplace: false });
  const [input, setInput] = useState("");
  const [language, setLanguage] = useState("");
  const [view, setView] = useState<View>({ kind: "menu" });
  const [copied, setCopied] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const resultRef = useRef<HTMLTextAreaElement>(null);
  const requestId = useRef(0);

  const reset = useCallback((p: PopupPayload) => {
    requestId.current++;
    setPayload(p);
    setInput(p.text);
    setView({ kind: "menu" });
    setCopied(false);
  }, []);

  useEffect(() => {
    void api.getCatalog().then(setCatalog);
    void api.popupState().then(reset);
    const un = listen<PopupPayload>("popup:open", (e) => reset(e.payload));
    return () => void un.then((f) => f());
  }, [reset]);

  useEffect(() => {
    if (settings) setLanguage((l) => l || settings.targetLanguage);
  }, [settings]);

  // Fit the native window to the card so the transparent margin stays minimal.
  useLayoutEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      void getCurrentWindow().setSize(new LogicalSize(WIDTH, Math.ceil(el.getBoundingClientRect().height)));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const actions = (catalog?.actions ?? []).filter((a) => a.id !== "translate" && !settings?.hiddenActions.includes(a.id));
  const showTranslate = !settings?.hiddenActions.includes("translate");
  const sounds = settings?.sounds ?? true;

  const run = useCallback(
    async (action: string) => {
      const text = input.trim() ? input : payload.text;
      if (!text.trim()) return;
      const id = ++requestId.current;
      playSound("click", sounds);
      setView({ kind: "working", action });
      try {
        const r = await api.runAction(action, text, action === "translate" ? language : undefined);
        if (id !== requestId.current) return;
        playSound("success", sounds);
        setView({ kind: "result", action, text: r.text });
      } catch (e) {
        if (id !== requestId.current) return;
        playSound("error", sounds);
        const { title, detail } = describeError(t, e);
        setView({ kind: "error", action, title, detail });
      }
    },
    [input, payload.text, language, sounds, t],
  );

  const close = useCallback(() => {
    requestId.current++;
    void api.popupClose();
  }, []);

  const apply = useCallback(async () => {
    if (view.kind !== "result") return;
    const text = resultRef.current?.value ?? view.text;
    await api.popupApply(text);
  }, [view]);

  const copy = useCallback(async () => {
    if (view.kind !== "result") return;
    await api.copyText(resultRef.current?.value ?? view.text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  }, [view]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (view.kind === "menu") close();
        else reset(payload);
        return;
      }
      const typing = (e.target as HTMLElement)?.tagName === "TEXTAREA" || (e.target as HTMLElement)?.tagName === "SELECT";
      if (view.kind === "result" && e.key === "Enter" && (!typing || e.ctrlKey)) {
        e.preventDefault();
        void apply();
      }
      if (view.kind === "menu" && !typing && /^[1-9]$/.test(e.key)) {
        const a = actions[Number(e.key) - 1];
        if (a) void run(a.id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [view, actions, run, apply, close, reset, payload]);

  return (
    <div ref={rootRef} className="p-1.5">
      <div className="bg-popover text-popover-foreground overflow-hidden rounded-xl border shadow-2xl">
        <header
          data-tauri-drag-region
          className="text-muted-foreground flex h-9 items-center gap-2 border-b px-3 text-[11px] font-semibold tracking-wider uppercase"
        >
          {view.kind !== "menu" && (
            <button
              type="button"
              className="hover:text-foreground -ml-1 rounded p-0.5"
              onClick={() => reset(payload)}
              title={t("common.back")}
            >
              <ArrowLeft className="size-3.5" />
            </button>
          )}
          {view.kind === "menu" && <img src="/app-icon.svg" alt="" className="pointer-events-none size-4 rounded" />}
          <span data-tauri-drag-region>Kaku AI Writer</span>
          <div className="ml-auto flex items-center gap-1">
            <button
              type="button"
              className="hover:text-foreground rounded p-0.5"
              title={t("popup.settings")}
              onClick={() => void api.openSettings()}
            >
              <Settings className="size-3.5" />
            </button>
            <button type="button" className="hover:text-foreground rounded p-0.5" title={t("common.close")} onClick={close}>
              <X className="size-3.5" />
            </button>
          </div>
        </header>

        <AnimatePresence mode="popLayout" initial={false}>
          <motion.div
            key={view.kind}
            initial={{ opacity: 0, x: view.kind === "menu" ? -12 : 12, filter: "blur(2px)" }}
            animate={{ opacity: 1, x: 0, filter: "blur(0px)" }}
            exit={{ opacity: 0, filter: "blur(2px)" }}
            transition={{ type: "spring", stiffness: 420, damping: 36 }}
          >
            {view.kind === "menu" && (
              <div className="p-2">
                {!payload.hasSelection && (
                  <textarea
                    autoFocus
                    className="bg-muted/40 placeholder:text-muted-foreground mb-2 h-20 w-full resize-none rounded-md border p-2 text-sm outline-none select-text"
                    placeholder={payload.canReplace ? t("popup.placeholderNoCopy") : t("popup.placeholder")}
                    value={input}
                    onChange={(e) => setInput(e.target.value)}
                  />
                )}
                <div className="grid grid-cols-2 gap-0.5">
                  <Highlight
                    hover
                    click={false}
                    className="bg-accent rounded-md"
                    transition={{ type: "spring", stiffness: 500, damping: 38 }}
                  >
                    {actions.map((a, i) => {
                      const Icon = ACTION_ICONS[a.id];
                      return (
                        <button
                          key={a.id}
                          data-value={a.id}
                          type="button"
                          autoFocus={payload.hasSelection && i === 0}
                          onClick={() => void run(a.id)}
                          className="focus-visible:ring-ring/60 group flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-sm outline-none focus-visible:ring-2"
                        >
                          {Icon && <Icon className="text-muted-foreground group-hover:text-foreground size-4" />}
                          <span className="truncate">{actionLabel(t, a.id)}</span>
                          {i < 9 && <span className="text-muted-foreground/60 ml-auto text-[10px] tabular-nums">{i + 1}</span>}
                        </button>
                      );
                    })}
                  </Highlight>
                </div>
                {showTranslate && (
                  <div className="bg-muted/50 mt-2 flex items-center gap-2 rounded-lg p-1">
                    <button
                      type="button"
                      onClick={() => void run("translate")}
                      className="hover:bg-accent flex flex-1 items-center justify-center gap-2 rounded-md py-1.5 text-sm font-medium"
                    >
                      <Globe className="size-4" />
                      {t("popup.translateTo")}
                    </button>
                    <select
                      aria-label={t("general.targetLanguage")}
                      className="text-muted-foreground hover:text-foreground max-w-36 cursor-pointer rounded-md bg-transparent px-1 py-1 text-xs outline-none [&>option]:bg-popover"
                      value={language}
                      onChange={(e) => setLanguage(e.target.value)}
                    >
                      {catalog?.languages.map((l) => (
                        <option key={l} value={l}>
                          {l}
                        </option>
                      ))}
                    </select>
                  </div>
                )}
                <p className="text-muted-foreground/70 mt-2 text-center text-[10px]">{t("popup.hint")}</p>
              </div>
            )}

            {view.kind === "working" && (
              <div className="text-muted-foreground flex h-40 flex-col items-center justify-center gap-3 text-sm">
                <Loader2 className="size-5 animate-spin" />
                {t("popup.working", { action: actionLabel(t, view.action) })}
              </div>
            )}

            {view.kind === "result" && (
              <div className="p-2">
                <label className="text-muted-foreground mb-1 block px-1 text-[11px]">{t("popup.result")}</label>
                <textarea
                  ref={resultRef}
                  key={view.text}
                  defaultValue={view.text}
                  autoFocus
                  className="bg-muted/40 h-44 w-full resize-none rounded-md border p-2 text-sm leading-relaxed outline-none select-text"
                />
                <p className="text-muted-foreground px-1 pt-1 text-[11px]">
                  {payload.canReplace
                    ? payload.hasSelection
                      ? t("popup.replaceHint")
                      : t("popup.replaceHintNoCopy")
                    : t("popup.noTarget")}
                </p>
                <div className="mt-2 flex items-center gap-1.5">
                  <button
                    type="button"
                    onClick={() => void run(view.action)}
                    className="hover:bg-accent text-muted-foreground hover:text-foreground rounded-md p-2"
                    title={t("common.retry")}
                  >
                    <RotateCcw className="size-4" />
                  </button>
                  <button
                    type="button"
                    onClick={() => void copy()}
                    className="hover:bg-accent flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-sm"
                  >
                    {copied ? <Check className="size-4 text-emerald-500" /> : <ClipboardCopy className="size-4" />}
                    {copied ? t("common.copied") : t("popup.copyResult")}
                  </button>
                  <button
                    type="button"
                    onClick={() => void apply()}
                    className={cn(
                      "bg-primary text-primary-foreground hover:bg-primary/90 ml-auto flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium",
                    )}
                  >
                    {payload.canReplace ? <Replace className="size-4" /> : <Check className="size-4" />}
                    {payload.canReplace ? t("popup.replace") : t("popup.copyClose")}
                  </button>
                </div>
              </div>
            )}

            {view.kind === "error" && (
              <div className="p-3">
                <div className="text-destructive text-sm font-medium">{view.title}</div>
                <p className="text-muted-foreground mt-1.5 max-h-32 overflow-y-auto text-xs whitespace-pre-line select-text">
                  {view.detail}
                </p>
                <div className="mt-3 flex gap-1.5">
                  <button
                    type="button"
                    onClick={() => void run(view.action)}
                    className="hover:bg-accent flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-sm"
                  >
                    <RotateCcw className="size-4" /> {t("common.retry")}
                  </button>
                  <button
                    type="button"
                    onClick={() => void api.openSettings("provider")}
                    className="hover:bg-accent flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm"
                  >
                    <Settings className="size-4" /> {t("popup.settings")}
                  </button>
                </div>
              </div>
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}
