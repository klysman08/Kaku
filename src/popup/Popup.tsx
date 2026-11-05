import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import {
  ArrowLeft,
  Check,
  ClipboardCopy,
  Globe,
  Loader2,
  Lock,
  LockOpen,
  Mic,
  Pin,
  PinOff,
  Replace,
  RotateCcw,
  Settings,
  X,
} from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { Highlight } from "@animate-ui/react/primitives/effects/highlight";
import { ACTION_ICONS, actionLabel, describeError } from "@/lib/actions";
import { api, type Catalog, type PopupPayload, type PopupPrefs } from "@/lib/ipc";
import { playSound } from "@/lib/sound";
import { useLiveSettings } from "@/lib/useSettings";
import { cn } from "@/lib/utils";
import { appendText, useDictation } from "./useDictation";

type View =
  | { kind: "menu" }
  | { kind: "working"; action: string }
  | { kind: "result"; action: string; text: string }
  | { kind: "error"; action: string; title: string; detail: string };

const WIDTH = 340;
const EMPTY: PopupPayload = { text: "", hasSelection: false, canReplace: false, capturing: false };

const headerButton = "hover:text-foreground hover:bg-accent rounded p-1 transition-colors";

/** Mouse-down on the header (not on its buttons) drags the window. */
function dragWindow(e: React.MouseEvent) {
  if (e.button !== 0 || (e.target as HTMLElement).closest("button, select, textarea, input")) return;
  e.preventDefault();
  void getCurrentWindow().startDragging();
}

export function Popup() {
  const { t } = useTranslation();
  const settings = useLiveSettings();
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [payload, setPayload] = useState<PopupPayload>(EMPTY);
  const [prefs, setPrefs] = useState<PopupPrefs>({ keepOpen: false, lockPosition: false });
  const [applied, setApplied] = useState(false);
  const [input, setInput] = useState("");
  const [language, setLanguage] = useState("");
  const [view, setView] = useState<View>({ kind: "menu" });
  const [copied, setCopied] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const resultRef = useRef<HTMLTextAreaElement>(null);
  const requestId = useRef(0);
  // Latest text-box value, also updated by dictation before React re-renders.
  const inputRef = useRef("");
  const changeInput = useCallback((value: string) => {
    inputRef.current = value;
    setInput(value);
  }, []);
  const dictation = useDictation(useCallback((text: string) => changeInput(appendText(inputRef.current, text)), [changeInput]));
  const { cancel: cancelDictation, clearError: clearDictationError } = dictation;

  const reset = useCallback(
    (p: PopupPayload) => {
      requestId.current++;
      cancelDictation();
      clearDictationError();
      setPayload(p);
      changeInput(p.text);
      setView({ kind: "menu" });
      setCopied(false);
      if (p.text || p.capturing) setApplied(false);
    },
    [cancelDictation, clearDictationError, changeInput],
  );

  useEffect(() => {
    void api.getCatalog().then(setCatalog);
    void api.popupState().then(reset);
    void api.popupPrefs().then(setPrefs);
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
      // Clicking an action while dictating uses what was said so far.
      if (dictation.active) await dictation.finish();
      const current = inputRef.current;
      const text = current.trim() ? current : payload.text;
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
    [dictation, payload.text, language, sounds, t],
  );

  const close = useCallback(() => {
    requestId.current++;
    void api.popupClose();
  }, []);

  const apply = useCallback(async () => {
    if (view.kind !== "result") return;
    const text = resultRef.current?.value ?? view.text;
    await api.popupApply(text);
    // A palette kept open stays up, ready for the next selection.
    if (prefs.keepOpen) {
      reset(EMPTY);
      setApplied(true);
    }
  }, [view, prefs.keepOpen, reset]);

  const togglePref = useCallback(async (patch: Partial<PopupPrefs>) => {
    setPrefs(await api.popupSetPrefs(patch));
  }, []);

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
        if (dictation.active) dictation.cancel();
        else if (view.kind === "menu") close();
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
  }, [view, actions, run, apply, close, reset, payload, dictation]);

  return (
    <div ref={rootRef} className="p-1.5">
      <div className="bg-popover text-popover-foreground overflow-hidden rounded-xl border shadow-2xl">
        <header
          onMouseDown={dragWindow}
          title={t("popup.drag")}
          className="text-muted-foreground flex h-9 cursor-grab items-center gap-2 border-b px-3 text-[11px] font-semibold tracking-wider uppercase select-none active:cursor-grabbing"
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
          <span>Kaku AI Writer</span>
          <div className="ml-auto flex items-center gap-0.5">
            <button
              type="button"
              aria-pressed={prefs.keepOpen}
              className={cn(headerButton, prefs.keepOpen && "text-primary bg-accent")}
              title={prefs.keepOpen ? t("popup.keepOpenOn") : t("popup.keepOpen")}
              onClick={() => void togglePref({ keepOpen: !prefs.keepOpen })}
            >
              {prefs.keepOpen ? <Pin className="size-3.5" /> : <PinOff className="size-3.5" />}
            </button>
            <button
              type="button"
              aria-pressed={prefs.lockPosition}
              className={cn(headerButton, prefs.lockPosition && "text-primary bg-accent")}
              title={prefs.lockPosition ? t("popup.lockPositionOn") : t("popup.lockPosition")}
              onClick={() => void togglePref({ lockPosition: !prefs.lockPosition })}
            >
              {prefs.lockPosition ? <Lock className="size-3.5" /> : <LockOpen className="size-3.5" />}
            </button>
            <button type="button" className={headerButton} title={t("popup.settings")} onClick={() => void api.openSettings()}>
              <Settings className="size-3.5" />
            </button>
            <button type="button" className={headerButton} title={t("common.close")} onClick={close}>
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
                {applied && <p className="text-muted-foreground mb-2 px-1 text-xs">{t("popup.applied")}</p>}
                {payload.capturing ? (
                  <div className="bg-muted/40 text-muted-foreground mb-2 flex h-20 items-center justify-center gap-2 rounded-md border text-sm">
                    <Loader2 className="size-4 animate-spin" />
                    {t("popup.capturing")}
                  </div>
                ) : (
                  !payload.hasSelection && (
                    <div className="mb-2">
                      <div className="relative">
                        <textarea
                          autoFocus
                          className="bg-muted/40 placeholder:text-muted-foreground block h-20 w-full resize-none rounded-md border p-2 pr-10 text-sm outline-none select-text"
                          placeholder={payload.canReplace ? t("popup.placeholderNoCopy") : t("popup.placeholder")}
                          value={input}
                          readOnly={dictation.state === "transcribing"}
                          onChange={(e) => changeInput(e.target.value)}
                        />
                        <MicButton
                          state={dictation.state}
                          level={dictation.level}
                          label={dictation.active ? t("popup.stopDictation") : t("popup.dictate")}
                          onClick={dictation.toggle}
                        />
                      </div>
                      {dictation.state === "listening" && (
                        <p className="text-muted-foreground mt-1 truncate px-1 text-[11px]">
                          {dictation.partial ? <span className="italic">{dictation.partial}</span> : t("popup.listening")}
                        </p>
                      )}
                      {dictation.state === "transcribing" && (
                        <p className="text-muted-foreground mt-1 px-1 text-[11px]">{t("popup.transcribing")}</p>
                      )}
                      {dictation.error && (
                        <div className="text-destructive mt-1 px-1 text-[11px] leading-snug">
                          <span className="font-medium">{dictation.error.title}.</span> {dictation.error.detail}
                          {dictation.error.code === "speech_privacy" && (
                            <button
                              type="button"
                              className="text-foreground ml-1 underline underline-offset-2"
                              onClick={() => void api.openSpeechSettings()}
                            >
                              {t("popup.openSpeechSettings")}
                            </button>
                          )}
                        </div>
                      )}
                    </div>
                  )
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

/** Microphone toggle inside the text box; the ring follows the input level. */
function MicButton({
  state,
  level,
  label,
  onClick,
}: {
  state: "idle" | "starting" | "listening" | "transcribing";
  level: number;
  label: string;
  onClick: () => void;
}) {
  const listening = state === "listening";
  const busy = state === "starting" || state === "transcribing";
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={listening}
      disabled={busy}
      onClick={onClick}
      className={cn(
        "absolute right-1.5 bottom-1.5 grid size-7 place-items-center rounded-full transition-colors",
        listening ? "bg-red-500 text-white" : "text-muted-foreground hover:text-foreground hover:bg-accent",
      )}
    >
      {listening && (
        <motion.span
          aria-hidden
          className="absolute inset-0 rounded-full bg-red-500/40"
          animate={{ scale: 1 + Math.min(level, 1) * 0.7 }}
          transition={{ type: "spring", stiffness: 300, damping: 20 }}
        />
      )}
      {busy ? <Loader2 className="relative size-3.5 animate-spin" /> : <Mic className="relative size-3.5" />}
    </button>
  );
}
