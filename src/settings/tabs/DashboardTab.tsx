import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/form";
import { SimpleSelect } from "@/components/ui/simple-select";
import { CountingNumber } from "@animate-ui/react/primitives/texts/counting-number";
import { motion } from "motion/react";
import { ACTION_ICONS, actionLabel } from "@/lib/actions";
import { estimateCost, fillDays, formatNumber, formatUsd } from "@/lib/cost";
import { api, type KeyBucket, type UsageSummary } from "@/lib/ipc";
import type { TabProps } from "../App";
import { Notice, Row, Section } from "../components";

const PERIODS = [7, 30, 90, 0];

function Stat({ label, value, hint, index }: { label: string; value: React.ReactNode; hint?: string; index: number }) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: index * 0.05, type: "spring", stiffness: 300, damping: 30 }}
      className="bg-card rounded-xl border p-4"
    >
      <div className="text-muted-foreground text-xs font-medium">{label}</div>
      <div className="mt-1.5 text-2xl font-semibold tabular-nums tracking-tight">{value}</div>
      {hint && <div className="text-muted-foreground mt-1 text-xs">{hint}</div>}
    </motion.div>
  );
}

/** Animated count-up (Animate UI) for integers; very large values use compact text. */
function Count({ n, locale }: { n: number; locale: string }) {
  if (n >= 100_000) return <>{formatNumber(n, locale)}</>;
  const sep = new Intl.NumberFormat(locale).format(1000).charAt(1);
  return <CountingNumber number={n} thousandSeparator={/[0-9]/.test(sep) ? "" : sep} transition={{ stiffness: 120, damping: 30 }} />;
}

function Bars({ rows, label, locale }: { rows: KeyBucket[]; label: (k: string) => React.ReactNode; locale: string }) {
  const max = Math.max(1, ...rows.map((r) => r.requests));
  return (
    <div className="space-y-2.5 p-4">
      {rows.map((r) => (
        <div key={r.key} className="grid grid-cols-[10rem_1fr_3rem] items-center gap-3 text-sm">
          <span className="truncate">{label(r.key)}</span>
          <div className="bg-muted h-2 overflow-hidden rounded-full">
            <motion.div
              className="bg-primary h-full rounded-full"
              initial={{ width: 0 }}
              animate={{ width: `${(r.requests / max) * 100}%` }}
              transition={{ type: "spring", stiffness: 140, damping: 24 }}
            />
          </div>
          <span className="text-muted-foreground text-right tabular-nums">{formatNumber(r.requests, locale)}</span>
        </div>
      ))}
    </div>
  );
}

export function DashboardTab({ draft, update }: TabProps) {
  const { t, i18n } = useTranslation();
  const locale = i18n.language;
  const [days, setDays] = useState(30);
  const [data, setData] = useState<UsageSummary | null>(null);

  const load = useCallback(() => void api.usageSummary(days).then(setData), [days]);
  useEffect(() => {
    load();
    const un = listen("usage-changed", load);
    return () => void un.then((f) => f());
  }, [load]);

  const chart = useMemo(() => {
    if (!data) return [];
    const span = days || Math.min(90, Math.max(7, data.byDay.length));
    const rows = days ? data.byDay : data.byDay.slice(-span);
    return fillDays(rows, span, (day) => ({ day, requests: 0, inputTokens: 0, outputTokens: 0 })).map((r) => ({
      ...r,
      label: new Date(`${r.day}T00:00`).toLocaleDateString(locale, { day: "2-digit", month: "2-digit" }),
    }));
  }, [data, days, locale]);

  if (!data) return null;
  const { totals } = data;
  const cost = estimateCost(data.byModel, draft.pricing);
  const errorRate = totals.requests ? Math.round((totals.errors / totals.requests) * 100) : 0;

  async function reset() {
    if (!window.confirm(t("common.confirm"))) return;
    await api.resetUsage();
    load();
  }

  const setPrice = (model: string, field: "input" | "output", value: number) => {
    const current = draft.pricing[model] ?? { input: 0, output: 0 };
    update({ pricing: { ...draft.pricing, [model]: { ...current, [field]: value } } });
  };

  return (
    <div>
      <div className="mb-5 flex items-center justify-between">
        <SimpleSelect
          aria-label={t("dashboard.period")}
          value={String(days)}
          onValueChange={(v) => setDays(Number(v))}
          options={PERIODS.map((d) => ({ value: String(d), label: d ? t("dashboard.last", { count: d }) : t("dashboard.allTime") }))}
        />
        <Button variant="ghost" size="sm" onClick={reset}>
          <RotateCcw /> {t("dashboard.reset")}
        </Button>
      </div>

      <div className="mb-6 grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat
          index={0}
          label={t("dashboard.requests")}
          value={<Count n={totals.requests} locale={locale} />}
          hint={t("dashboard.errorRate", { rate: errorRate })}
        />
        <Stat
          index={1}
          label={t("dashboard.tokens")}
          value={<Count n={totals.inputTokens + totals.outputTokens} locale={locale} />}
          hint={`${t("dashboard.input")} ${formatNumber(totals.inputTokens, locale)} · ${t("dashboard.output")} ${formatNumber(totals.outputTokens, locale)}`}
        />
        <Stat index={2} label={t("dashboard.latency")} value={`${(totals.avgLatencyMs / 1000).toFixed(1)} s`} />
        <Stat
          index={3}
          label={t("dashboard.cost")}
          value={formatUsd(cost, locale)}
          hint={Object.keys(draft.pricing).length ? undefined : t("dashboard.costHint")}
        />
      </div>

      {totals.requests === 0 ? (
        <Notice>{t("dashboard.empty")}</Notice>
      ) : (
        <>
          <Section title={t("dashboard.perDay")}>
            <div className="h-56 p-4">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={chart} margin={{ top: 4, right: 4, bottom: 0, left: -12 }}>
                  <CartesianGrid vertical={false} stroke="var(--border)" />
                  <XAxis
                    dataKey="label"
                    tickLine={false}
                    axisLine={false}
                    fontSize={11}
                    stroke="var(--muted-foreground)"
                    interval="preserveStartEnd"
                    minTickGap={16}
                  />
                  <YAxis tickLine={false} axisLine={false} fontSize={11} stroke="var(--muted-foreground)" allowDecimals={false} />
                  <Tooltip
                    cursor={{ fill: "var(--accent)" }}
                    contentStyle={{ background: "var(--popover)", border: "1px solid var(--border)", borderRadius: 8, fontSize: 12 }}
                    labelStyle={{ color: "var(--popover-foreground)" }}
                  />
                  <Bar dataKey="requests" name={t("dashboard.requests")} fill="var(--chart-1)" radius={[4, 4, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
            </div>
          </Section>

          <div className="grid gap-x-6 lg:grid-cols-2">
            <Section title={t("dashboard.byAction")}>
              <Bars
                rows={data.byAction}
                locale={locale}
                label={(k) => {
                  const Icon = ACTION_ICONS[k];
                  return (
                    <span className="inline-flex items-center gap-2">
                      {Icon && <Icon className="text-muted-foreground size-3.5" />}
                      {actionLabel(t, k)}
                    </span>
                  );
                }}
              />
            </Section>
            <Section title={t("dashboard.byModel")}>
              <Bars rows={data.byModel} locale={locale} label={(k) => <span className="font-mono text-xs">{k}</span>} />
            </Section>
          </div>
        </>
      )}

      <Section title={t("dashboard.pricing")} description={t("dashboard.pricingDesc")}>
        {[...new Set([draft.provider === "gemini" ? draft.geminiModel : draft.openaiModel, ...data.byModel.map((m) => m.key)])].map(
          (model) => (
            <Row key={model} label={<span className="font-mono text-xs">{model}</span>}>
              <div className="flex items-center gap-2 text-xs">
                <span className="text-muted-foreground">{t("dashboard.priceInput")}</span>
                <Input
                  type="number"
                  min={0}
                  step="0.01"
                  className="h-8 w-24"
                  value={draft.pricing[model]?.input ?? ""}
                  onChange={(e) => setPrice(model, "input", Number(e.target.value))}
                />
                <span className="text-muted-foreground ml-2">{t("dashboard.priceOutput")}</span>
                <Input
                  type="number"
                  min={0}
                  step="0.01"
                  className="h-8 w-24"
                  value={draft.pricing[model]?.output ?? ""}
                  onChange={(e) => setPrice(model, "output", Number(e.target.value))}
                />
              </div>
            </Row>
          ),
        )}
      </Section>
    </div>
  );
}
