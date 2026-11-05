import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Loader2 } from "lucide-react";
import "@/styles.css";
import "@/i18n";
import { actionLabel } from "@/lib/actions";
import { useLiveSettings } from "@/lib/useSettings";

document.documentElement.classList.add("transparent");

/** Non-focusable progress pill shown next to the cursor while a direct shortcut runs. */
function Toast() {
  const { t } = useTranslation();
  useLiveSettings();
  const [action, setAction] = useState("");

  useEffect(() => {
    const un = listen<string>("toast:show", (e) => setAction(e.payload));
    return () => void un.then((f) => f());
  }, []);

  return (
    <div className="p-1.5">
      <div className="bg-popover text-popover-foreground flex h-9 items-center gap-2.5 rounded-full border px-4 text-sm shadow-lg">
        <Loader2 className="size-4 animate-spin" />
        <span className="truncate">{t("toast.working", { action: action ? actionLabel(t, action) : "Kaku" })}</span>
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Toast />
  </React.StrictMode>,
);
