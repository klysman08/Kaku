import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** A titled group of settings rows. */
export function Section({
  title,
  description,
  children,
  className,
}: {
  title: string;
  description?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("mb-8", className)}>
      <h2 className="text-sm font-semibold">{title}</h2>
      {description && <p className="text-muted-foreground mt-1 text-sm">{description}</p>}
      <div className="bg-card mt-3 divide-y rounded-xl border">{children}</div>
    </section>
  );
}

/** Label + description on the left, control on the right. */
export function Row({
  label,
  description,
  children,
  stack,
}: {
  label: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  stack?: boolean;
}) {
  return (
    <div className={cn("flex gap-4 px-4 py-3.5", stack ? "flex-col" : "items-center justify-between")}>
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        {description && <div className="text-muted-foreground mt-0.5 text-xs leading-relaxed">{description}</div>}
      </div>
      {children && <div className={cn(stack ? "w-full" : "shrink-0")}>{children}</div>}
    </div>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="bg-muted text-foreground inline-flex h-6 min-w-6 items-center justify-center rounded border px-1.5 font-mono text-[11px] font-medium shadow-[inset_0_-1px_0_var(--border)]">
      {children}
    </kbd>
  );
}

export function Notice({
  tone = "info",
  title,
  children,
}: {
  tone?: "info" | "warn" | "error" | "ok";
  title?: ReactNode;
  children?: ReactNode;
}) {
  const tones = {
    info: "border-border bg-muted/40",
    warn: "border-amber-500/40 bg-amber-500/10",
    error: "border-destructive/40 bg-destructive/10",
    ok: "border-emerald-500/40 bg-emerald-500/10",
  };
  return (
    <div className={cn("rounded-lg border px-4 py-3 text-sm", tones[tone])}>
      {title && <div className="font-medium">{title}</div>}
      {children && <div className={cn("text-muted-foreground whitespace-pre-line", title && "mt-1")}>{children}</div>}
    </div>
  );
}
