import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { cn } from "@/lib/utils";

const win = getCurrentWindow();

function Control({
  label,
  onClick,
  danger,
  children,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn(
        "text-muted-foreground grid h-full w-11 place-items-center transition-colors [&_svg]:size-3.5",
        danger ? "hover:bg-red-500 hover:text-white" : "hover:bg-accent hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

/**
 * Frameless window chrome: the whole bar is a drag region (double-click maximizes);
 * minimize/close follow the "minimize to tray" setting on the Rust side.
 */
export function TitleBar({ title, minimizeToTray }: { title: string; minimizeToTray: boolean }) {
  const { t } = useTranslation();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    void win.isMaximized().then(setMaximized);
    const un = win.onResized(() => void win.isMaximized().then(setMaximized));
    return () => void un.then((f) => f());
  }, []);

  return (
    <div data-tauri-drag-region className="flex h-10 shrink-0 items-stretch select-none">
      <div data-tauri-drag-region className="text-muted-foreground flex flex-1 items-center px-8 text-xs font-medium">
        {title}
      </div>
      <Control label={minimizeToTray ? t("window.minimizeToTray") : t("window.minimize")} onClick={() => void invoke("window_minimize")}>
        <Minus />
      </Control>
      <Control label={maximized ? t("window.restore") : t("window.maximize")} onClick={() => void win.toggleMaximize()}>
        {maximized ? <Copy className="-scale-x-100" /> : <Square />}
      </Control>
      <Control label={minimizeToTray ? t("window.closeToTray") : t("window.close")} onClick={() => void invoke("window_close")} danger>
        <X />
      </Control>
    </div>
  );
}
