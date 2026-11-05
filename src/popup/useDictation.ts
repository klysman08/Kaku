import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { describeError } from "@/lib/actions";
import { api, type CommandError } from "@/lib/ipc";

export type DictationState = "idle" | "starting" | "listening" | "transcribing";

export interface DictationError {
  code: string;
  title: string;
  detail: string;
}

/** Join dictated text onto what is already in the box. */
export function appendText(current: string, addition: string): string {
  const add = addition.trim();
  if (!add) return current;
  if (!current) return add;
  return /\s$/.test(current) ? current + add : `${current} ${add}`;
}

/**
 * Microphone dictation for the palette text box. `onText` receives each finished piece:
 * phrase by phrase with the native engine, the whole transcript with the AI engine.
 */
export function useDictation(onText: (text: string) => void) {
  const { t } = useTranslation();
  const [state, setState] = useState<DictationState>("idle");
  const [partial, setPartial] = useState("");
  const [level, setLevel] = useState(0);
  const [error, setError] = useState<DictationError | null>(null);
  const stateRef = useRef<DictationState>("idle");
  const onTextRef = useRef(onText);
  onTextRef.current = onText;

  const set = useCallback((s: DictationState) => {
    stateRef.current = s;
    setState(s);
    if (s === "idle") {
      setPartial("");
      setLevel(0);
    }
  }, []);

  const fail = useCallback(
    (e: unknown) => {
      const { title, detail } = describeError(t, e);
      const code = typeof e === "object" && e && "code" in e ? String((e as CommandError).code) : "";
      setError({ code, title, detail });
    },
    [t],
  );

  const finish = useCallback(async () => {
    if (stateRef.current !== "listening") return;
    set("transcribing");
    try {
      const text = await api.speechStop();
      if (text) onTextRef.current(text);
    } catch (e) {
      fail(e);
    } finally {
      set("idle");
    }
  }, [fail, set]);

  useEffect(() => {
    const subs = [
      listen<string>("speech:partial", (e) => setPartial(e.payload)),
      listen<string>("speech:final", (e) => {
        setPartial("");
        onTextRef.current(e.payload);
      }),
      listen<number>("speech:level", (e) => setLevel(e.payload)),
      listen<CommandError>("speech:error", (e) => fail(e.payload)),
      // The native session ended by itself (silence, error).
      listen<string>("speech:state", (e) => {
        if (e.payload === "idle" && stateRef.current === "listening") set("idle");
      }),
      listen("speech:limit", () => void finish()),
    ];
    return () => subs.forEach((p) => void p.then((f) => f()));
  }, [fail, finish, set]);

  const start = useCallback(async () => {
    if (stateRef.current !== "idle") return;
    setError(null);
    set("starting");
    try {
      await api.speechStart();
      set("listening");
    } catch (e) {
      fail(e);
      set("idle");
    }
  }, [fail, set]);

  const toggle = useCallback(() => {
    if (stateRef.current === "idle") void start();
    else if (stateRef.current === "listening") void finish();
  }, [start, finish]);

  /** Drop the recording without text (Esc, palette reset). */
  const cancel = useCallback(() => {
    if (stateRef.current === "idle") return;
    void api.speechCancel();
    set("idle");
  }, [set]);

  const clearError = useCallback(() => setError(null), []);

  return { state, partial, level, error, toggle, finish, cancel, clearError, active: state !== "idle" };
}
