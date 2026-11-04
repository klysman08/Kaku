import type { ReactNode } from "react";
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from "./select";
import { cn } from "@/lib/utils";

export interface Option {
  value: string;
  label: ReactNode;
  hint?: ReactNode;
}

/** Animated (Animate UI + Radix) select for a flat list of options, optionally grouped. */
export function SimpleSelect({
  value,
  onValueChange,
  options,
  groups,
  placeholder,
  className,
  disabled,
  "aria-label": ariaLabel,
}: {
  value: string;
  onValueChange: (v: string) => void;
  options?: Option[];
  groups?: { label?: ReactNode; options: Option[] }[];
  placeholder?: string;
  className?: string;
  disabled?: boolean;
  "aria-label"?: string;
}) {
  const all = groups ?? [{ options: options ?? [] }];
  return (
    <Select value={value} onValueChange={onValueChange} disabled={disabled}>
      <SelectTrigger className={cn("w-52", className)} aria-label={ariaLabel}>
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent position="popper" className="max-h-80">
        {all.map((g, i) => (
          <SelectGroup key={i}>
            {i > 0 && <SelectSeparator />}
            {g.label && <SelectLabel>{g.label}</SelectLabel>}
            {g.options.map((o) => (
              <SelectItem key={o.value} value={o.value}>
                <span className="truncate">{o.label}</span>
                {o.hint && <span className="text-muted-foreground ml-2 truncate text-xs">{o.hint}</span>}
              </SelectItem>
            ))}
          </SelectGroup>
        ))}
      </SelectContent>
    </Select>
  );
}
