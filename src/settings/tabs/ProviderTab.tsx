import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CheckCircle2, ExternalLink, KeyRound, Loader2, PlugZap, RefreshCw, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/form";
import { SimpleSelect } from "@/components/ui/simple-select";
import { api, type KeyStatus, type ModelInfo, type ProviderKind } from "@/lib/ipc";
import { describeError } from "@/lib/actions";
import { cn } from "@/lib/utils";
import type { TabProps } from "../App";
import { Notice, Row, Section } from "../components";

const CUSTOM = "__custom__";

const PRESETS = [
  { name: "OpenAI", url: "https://api.openai.com/v1" },
  { name: "OpenRouter", url: "https://openrouter.ai/api/v1" },
  { name: "Ollama", url: "http://localhost:11434/v1" },
  { name: "LM Studio", url: "http://localhost:1234/v1" },
];

export function ProviderTab({ draft, saved, update, commit }: TabProps) {
  const { t } = useTranslation();
  const provider = draft.provider;
  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);

  const [keyStatus, setKeyStatus] = useState<KeyStatus | null>(null);
  const [newKey, setNewKey] = useState("");
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [busy, setBusy] = useState<"" | "key" | "models" | "test">("");
  const [customModel, setCustomModel] = useState(false);
  const [result, setResult] = useState<{ tone: "ok" | "error"; title: string; body?: string } | null>(null);

  // Load the model list automatically when the saved provider has a key (or is a local server).
  const canQuery = provider === saved.provider && (provider === "openai" || !!keyStatus?.configured);
  useEffect(() => {
    setResult(null);
    setModels([]);
    setCustomModel(false);
    void api.getApiKeyStatus(provider).then(setKeyStatus);
  }, [provider]);

  useEffect(() => {
    if (!canQuery || !keyStatus) return;
    api.listModels().then(setModels, () => setModels([]));
  }, [canQuery, keyStatus, saved.openaiBaseUrl]);

  async function run(kind: typeof busy, fn: () => Promise<void>) {
    setBusy(kind);
    setResult(null);
    try {
      await fn();
    } catch (e) {
      const { title, detail } = describeError(t, e);
      setResult({ tone: "error", title, body: detail });
    } finally {
      setBusy("");
    }
  }

  const saveKey = () =>
    run("key", async () => {
      setKeyStatus(await api.setApiKey(provider, newKey));
      setNewKey("");
    });

  const removeKey = () =>
    run("key", async () => {
      await api.deleteApiKey(provider);
      setKeyStatus({ configured: false, masked: null });
    });

  const loadModels = () =>
    run("models", async () => {
      const list = await api.listModels();
      setModels(list);
      setResult({ tone: "ok", title: t("provider.modelsLoaded", { count: list.length }) });
    });

  const test = () =>
    run("test", async () => {
      const r = await api.testConnection();
      setResult({ tone: "ok", title: t("provider.testOk", { ms: r.latencyMs, model: r.model }) });
    });

  const modelField = provider === "gemini" ? "geminiModel" : "openaiModel";

  return (
    <div>
      {!saved.onboarded && (
        <div className="mb-8">
          <Notice title={t("provider.welcomeTitle")}>
            <span>{t("provider.welcomeBody", { shortcut: saved.shortcuts.popup })} </span>
            <div className="mt-3">
              <Button size="sm" onClick={() => void commit({ onboarded: true })}>
                {t("provider.welcomeDone")}
              </Button>
            </div>
          </Notice>
        </div>
      )}

      <Section title={t("provider.provider")}>
        <div className="grid grid-cols-2 gap-3 p-4">
          {(["gemini", "openai"] as ProviderKind[]).map((p) => (
            <button
              key={p}
              type="button"
              onClick={() => update({ provider: p })}
              className={cn(
                "hover:bg-accent/50 rounded-lg border p-4 text-left transition-colors",
                provider === p && "border-primary ring-primary/20 ring-2",
              )}
            >
              <div className="text-sm font-medium">{t(`provider.${p}`)}</div>
              <div className="text-muted-foreground mt-1 text-xs leading-relaxed">
                {p === "gemini" ? "gemini-3.x · Google AI Studio" : t("provider.openaiDesc")}
              </div>
            </button>
          ))}
        </div>
      </Section>

      <Section title={provider === "openai" ? t("provider.apiKeyOptional") : t("provider.apiKey")} description={t("provider.apiKeyDesc")}>
        <div className="space-y-3 p-4">
          <div className="flex items-center gap-2 text-sm">
            <KeyRound className="text-muted-foreground size-4" />
            {keyStatus?.configured ? (
              <span>{t("provider.stored", { masked: keyStatus.masked })}</span>
            ) : (
              <span className="text-muted-foreground">{t("provider.notStored")}</span>
            )}
            {keyStatus?.configured && (
              <Button variant="ghost" size="sm" className="ml-auto" onClick={removeKey} disabled={busy === "key"}>
                <Trash2 /> {t("provider.removeKey")}
              </Button>
            )}
          </div>
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (newKey.trim()) void saveKey();
            }}
          >
            <Input
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder={t("provider.placeholder")}
              value={newKey}
              onChange={(e) => setNewKey(e.target.value)}
            />
            <Button type="submit" disabled={!newKey.trim() || busy === "key"}>
              {t("provider.saveKey")}
            </Button>
          </form>
          {provider === "gemini" && (
            <Button variant="link" className="h-auto p-0 text-xs" onClick={() => void openUrl("https://aistudio.google.com/apikey")}>
              {t("provider.getKey")} <ExternalLink className="size-3" />
            </Button>
          )}
        </div>
      </Section>

      <Section title={t("provider.model")}>
        {provider === "openai" && (
          <Row label={t("provider.baseUrl")} description={t("provider.baseUrlDesc")} stack>
            <Input value={draft.openaiBaseUrl} spellCheck={false} onChange={(e) => update({ openaiBaseUrl: e.target.value })} />
            <div className="mt-2 flex flex-wrap items-center gap-1.5">
              <span className="text-muted-foreground mr-1 text-xs">{t("provider.presets")}:</span>
              {PRESETS.map((p) => (
                <Button key={p.name} variant="outline" size="sm" className="h-7 text-xs" onClick={() => update({ openaiBaseUrl: p.url })}>
                  {p.name}
                </Button>
              ))}
            </div>
          </Row>
        )}
        <Row label={t("provider.model")} stack>
          <div className="flex gap-2">
            {customModel || models.length === 0 ? (
              <Input
                value={draft[modelField]}
                spellCheck={false}
                placeholder="model-id"
                onChange={(e) => update({ [modelField]: e.target.value } as Partial<typeof draft>)}
              />
            ) : (
              <SimpleSelect
                className="flex-1"
                aria-label={t("provider.model")}
                value={draft[modelField]}
                onValueChange={(v) => {
                  if (v === CUSTOM) setCustomModel(true);
                  else update({ [modelField]: v } as Partial<typeof draft>);
                }}
                groups={[
                  {
                    options: [
                      ...(models.some((m) => m.id === draft[modelField]) || !draft[modelField]
                        ? []
                        : [{ value: draft[modelField], label: draft[modelField] }]),
                      ...models.map((m) => ({ value: m.id, label: m.name, hint: m.name !== m.id ? m.id : undefined })),
                    ],
                  },
                  { options: [{ value: CUSTOM, label: t("provider.customModel") }] },
                ]}
              />
            )}
            {customModel && models.length > 0 && (
              <Button variant="ghost" onClick={() => setCustomModel(false)}>
                {t("common.back")}
              </Button>
            )}
            <Button variant="outline" onClick={loadModels} disabled={dirty || !!busy} title={dirty ? t("provider.saveFirst") : undefined}>
              {busy === "models" ? <Loader2 className="animate-spin" /> : <RefreshCw />}
              {t("provider.loadModels")}
            </Button>
          </div>
        </Row>
        <div className="flex items-center gap-3 px-4 py-3.5">
          <Button onClick={test} disabled={dirty || !!busy}>
            {busy === "test" ? <Loader2 className="animate-spin" /> : <PlugZap />}
            {t("provider.test")}
          </Button>
          {dirty && <span className="text-muted-foreground text-xs">{t("provider.saveFirst")}</span>}
        </div>
      </Section>

      {result && (
        <Notice
          tone={result.tone}
          title={
            <span className="inline-flex items-center gap-2">
              {result.tone === "ok" && <CheckCircle2 className="size-4 text-emerald-500" />}
              {result.title}
            </span>
          }
        >
          {result.body}
        </Notice>
      )}
    </div>
  );
}
