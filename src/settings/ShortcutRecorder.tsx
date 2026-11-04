import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { api } from "@/lib/ipc";
import { displayParts, recordKey } from "@/lib/shortcut";
import { cn } from "@/lib/utils";
import { Kbd } from "./components";

/**
 * Click, then press a combination. Global shortcuts are suspended while recording so the
 * combination being typed doesn't fire (they are re-applied on blur).
 */
export function ShortcutRecorder({ value, onChange, error }: { value: string; onChange: (v: string) => void; error?: string }) {
  const { t } = useTranslation();
  const [recording, setRecording] = useState(false);
  const [hint, setHint] = useState<string | null>(null);
  const ref = useRef<HTMLButtonElement>(null);

  const wasRecording = useRef(false);

  useEffect(() => {
    if (recording === wasRecording.current) return;
    wasRecording.current = recording;
    void api.suspendShortcuts(recording);
  }, [recording]);

  function onKeyDown(e: React.KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    const r = recordKey(e.nativeEvent);
    if (r.kind === "combo") {
      onChange(r.value);
      stop();
    } else if (r.kind === "clear") {
      onChange("");
      stop();
    } else if (r.kind === "cancel") stop();
    else if (r.kind === "needsModifier") setHint(t("shortcuts.needsModifier"));
  }

  function stop() {
    setRecording(false);
    setHint(null);
    ref.current?.blur();
  }

  return (
    <div className="flex flex-col items-end gap-1">
      <div className="flex items-center gap-1">
        <button
          ref={ref}
          type="button"
          onClick={() => setRecording(true)}
          onBlur={() => setRecording(false)}
          onKeyDown={onKeyDown}
          className={cn(
            "border-input hover:bg-accent/50 flex h-9 min-w-44 items-center justify-center gap-1 rounded-md border px-3 text-sm transition-colors outline-none",
            recording && "border-primary ring-primary/25 ring-[3px]",
            error && !recording && "border-destructive",
          )}
        >
          {recording ? (
            <span className="text-muted-foreground animate-pulse">{t("shortcuts.recording")}</span>
          ) : value ? (
            displayParts(value).map((p) => <Kbd key={p}>{p}</Kbd>)
          ) : (
            <span className="text-muted-foreground">{t("shortcuts.none")}</span>
          )}
        </button>
        {value && !recording && (
          <button
            type="button"
            className="text-muted-foreground hover:text-foreground rounded p-1"
            title={t("shortcuts.remove")}
            onClick={() => onChange("")}
          >
            <X className="size-4" />
          </button>
        )}
      </div>
      {(hint || error) && (
        <span className={cn("text-xs", error && !hint ? "text-destructive" : "text-muted-foreground")}>{hint ?? error}</span>
      )}
    </div>
  );
}
