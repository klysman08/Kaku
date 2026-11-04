import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge, Switch, Textarea } from "@/components/ui/form";
import { ACTION_ICONS, actionLabel } from "@/lib/actions";
import { cn } from "@/lib/utils";
import type { TabProps } from "../App";

const GUARD = "CRITICAL:";

export function ActionsTab({ draft, update, catalog }: TabProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState<string | null>(null);

  const setVisible = (id: string, visible: boolean) => {
    const hidden = new Set(draft.hiddenActions);
    if (visible) hidden.delete(id);
    else hidden.add(id);
    update({ hiddenActions: [...hidden] });
  };

  const setPrompt = (id: string, value: string | null) => {
    const customPrompts = { ...draft.customPrompts };
    if (value === null) delete customPrompts[id];
    else customPrompts[id] = value;
    update({ customPrompts });
  };

  return (
    <div>
      <p className="text-muted-foreground mb-4 text-sm">{t("actionsTab.desc")}</p>
      <div className="bg-card divide-y rounded-xl border">
        {catalog.actions.map((a) => {
          const Icon = ACTION_ICONS[a.id];
          const custom = draft.customPrompts[a.id];
          const hasCustom = custom !== undefined;
          const valid = !hasCustom || custom.includes(GUARD);
          const isOpen = open === a.id;
          return (
            <div key={a.id}>
              <div className="flex items-center gap-3 px-4 py-3">
                {Icon && <Icon className="text-muted-foreground size-4" />}
                <span className="text-sm font-medium">{actionLabel(t, a.id)}</span>
                {hasCustom && (
                  <Badge variant={valid ? "secondary" : "destructive"}>
                    {valid ? t("actionsTab.usingCustom") : t("actionsTab.invalid")}
                  </Badge>
                )}
                <div className="ml-auto flex items-center gap-3">
                  <label className="text-muted-foreground flex items-center gap-2 text-xs">
                    {t("actionsTab.visible")}
                    <Switch checked={!draft.hiddenActions.includes(a.id)} onCheckedChange={(v) => setVisible(a.id, v)} />
                  </label>
                  <Button variant="ghost" size="sm" onClick={() => setOpen(isOpen ? null : a.id)} aria-expanded={isOpen}>
                    {t("actionsTab.edit")}
                    <ChevronDown className={cn("transition-transform", isOpen && "rotate-180")} />
                  </Button>
                </div>
              </div>
              {isOpen && (
                <div className="space-y-2 px-4 pb-4">
                  <p className="text-muted-foreground text-xs">{t("actionsTab.customPromptDesc")}</p>
                  <Textarea
                    className="min-h-40 font-mono text-xs leading-relaxed"
                    spellCheck={false}
                    value={custom ?? a.defaultPrompt}
                    onChange={(e) => setPrompt(a.id, e.target.value === a.defaultPrompt ? null : e.target.value)}
                    aria-invalid={!valid}
                  />
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground text-xs">
                      {hasCustom ? t("actionsTab.usingCustom") : t("actionsTab.usingDefault")}
                    </span>
                    <Button variant="outline" size="sm" disabled={!hasCustom} onClick={() => setPrompt(a.id, null)}>
                      <RotateCcw /> {t("common.reset")}
                    </Button>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
