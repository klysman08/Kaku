import type { KeyBucket, ModelPrice } from "./ipc";

/** Estimated cost in USD from per-model prices (per 1M tokens). Unknown models cost 0. */
export function estimateCost(byModel: KeyBucket[], pricing: Record<string, ModelPrice>): number {
  return byModel.reduce((sum, m) => {
    const p = pricing[m.key];
    if (!p) return sum;
    return sum + (m.inputTokens * p.input + m.outputTokens * p.output) / 1_000_000;
  }, 0);
}

export function formatNumber(n: number, locale: string): string {
  return new Intl.NumberFormat(locale, { notation: n >= 100_000 ? "compact" : "standard", maximumFractionDigits: 1 }).format(n);
}

export function formatUsd(n: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: n < 1 ? 4 : 2,
  }).format(n);
}

/** Fill missing days so charts show gaps as zero instead of skipping them. */
export function fillDays<T extends { day: string }>(rows: T[], days: number, empty: (day: string) => T, today = new Date()): T[] {
  const byDay = new Map(rows.map((r) => [r.day, r]));
  const out: T[] = [];
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(today.getFullYear(), today.getMonth(), today.getDate() - i);
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    out.push(byDay.get(key) ?? empty(key));
  }
  return out;
}
