import React from "react";
import ReactDOM from "react-dom/client";
import { Sparkle } from "lucide-react";
import "@/styles.css";
import { useLiveSettings } from "@/lib/useSettings";

document.documentElement.classList.add("transparent");

/**
 * Button shown next to freshly selected text. The window never takes focus and the
 * click itself is handled by the mouse hook in `bubble.rs`, so the app that owns the
 * selection keeps it.
 */
function Bubble() {
  useLiveSettings();
  return (
    <div className="flex h-screen w-screen items-center justify-center">
      <div className="bg-popover text-popover-foreground hover:bg-accent flex size-7 cursor-pointer items-center justify-center rounded-full border shadow-md transition-colors">
        <Sparkle className="size-3.5" />
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Bubble />
  </React.StrictMode>,
);
