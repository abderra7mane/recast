import { memo, useCallback, useMemo, useRef, useState } from "react";
import { RotateCcw } from "lucide-react";

import type { CustomTheme } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { normalizeHex } from "@/theme/color";
import {
  contrastIssues,
  customTheme,
  MAX_NAME_LENGTH,
  originalColors,
  clampName,
  uniqueName,
} from "@/theme/theme";
import {
  TOKEN_GROUPS,
  TOKENS,
  type ThemeBase,
  type TokenKey,
} from "@/theme/tokens";

const TokenRow = memo(function TokenRow({
  token,
  label,
  value,
  original,
  onChange,
}: {
  token: TokenKey;
  label: string;
  value: string;
  original: string;
  onChange: (token: TokenKey, value: string) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const alpha = value.length === 9 ? value.slice(7) : "";
  const text = draft ?? value;
  return (
    <div className="flex items-center gap-1.5" data-token={token}>
      <span className="min-w-0 flex-1 truncate text-xs" title={label}>
        {label}
      </span>
      <input
        type="color"
        aria-label={`${label} color`}
        className="border-input h-7 w-9 shrink-0 cursor-pointer rounded border bg-transparent p-0.5"
        value={value.slice(0, 7)}
        onChange={(e) => onChange(token, e.target.value + alpha)}
      />
      <Input
        aria-label={`${label} hex`}
        aria-invalid={normalizeHex(text) === null}
        spellCheck={false}
        className="h-7 w-[5.75rem] shrink-0 px-1.5 font-mono text-xs md:text-xs"
        value={text}
        onChange={(e) => {
          setDraft(e.target.value);
          const color = normalizeHex(e.target.value);
          if (color) onChange(token, color);
        }}
        onBlur={() => setDraft(null)}
      />
      <Button
        size="icon"
        variant="ghost"
        className="size-7 shrink-0"
        aria-label={`Reset ${label}`}
        title="Reset"
        disabled={value === original}
        onClick={() => onChange(token, original)}
      >
        <RotateCcw />
      </Button>
    </div>
  );
});

/** Every themed control in a small mock-up, drawn with the live theme. */
export function ThemePreview() {
  return (
    <div
      inert
      data-testid="theme-preview"
      className="bg-background text-foreground space-y-3 rounded-lg border p-3"
    >
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm">Primary</Button>
        <Button size="sm" variant="outline">
          Outline
        </Button>
        <Button size="sm" variant="secondary">
          Secondary
        </Button>
        <Button size="sm" variant="destructive">
          Danger
        </Button>
        <Switch checked aria-label="On" />
        <Switch checked={false} aria-label="Off" />
        <Slider className="w-20" value={[60]} aria-label="Level" />
      </div>
      <p className="text-sm">
        Text · <span className="text-muted-foreground">secondary</span> ·{" "}
        <span className="text-brand">accent</span> ·{" "}
        <span className="text-warning">warning</span> ·{" "}
        <span className="text-success">done</span> ·{" "}
        <span className="text-destructive">error</span> ·{" "}
        <span className="bg-muted rounded px-1">subtle</span>{" "}
        <span className="bg-accent rounded px-1">hover</span>
      </p>
      <div className="bg-card relative space-y-1.5 rounded-md border p-2">
        <div className="relative h-6">
          <div className="bg-tl-recording ring-tl-recording-border absolute inset-y-0 right-0 left-0 rounded-md ring-1" />
          <div className="bg-tl-clip ring-tl-clip-border text-tl-clip-foreground absolute inset-y-0 right-[12%] left-[8%] rounded-md px-3 py-1 text-[11px] font-medium ring-1">
            Clip · 0:42
          </div>
          <div className="bg-tl-trim absolute inset-y-0 left-[8%] w-2 rounded-sm" />
          <div className="bg-tl-trim absolute inset-y-0 right-[12%] w-2 rounded-sm" />
          <div className="bg-tl-click absolute bottom-0.5 left-[30%] h-2 w-0.5 rounded-full" />
          <div className="bg-tl-click-alt absolute bottom-0.5 left-[46%] h-2 w-0.5 rounded-full" />
        </div>
        <div className="bg-tl-lane relative h-6 rounded-md">
          <div className="bg-tl-zoom text-tl-zoom-foreground ring-tl-zoom-border ring-tl-selected absolute inset-y-0 left-[14%] w-[24%] rounded-md text-center text-[11px] leading-6 font-medium ring-2">
            2.0×
          </div>
          <div className="bg-tl-zoom-auto text-tl-zoom-auto-foreground ring-tl-zoom-auto-border absolute inset-y-0 left-[52%] w-[30%] rounded-md text-center text-[11px] leading-6 font-medium ring-1">
            Auto 1.6×
          </div>
        </div>
        <div className="bg-tl-lane relative h-6 rounded-md">
          <div className="bg-tl-annotation text-tl-segment-foreground ring-tl-annotation-border absolute inset-y-0 left-[6%] w-[34%] rounded-md px-2 text-[11px] leading-6 font-medium ring-1">
            Arrow
          </div>
          <div className="bg-tl-obscure text-tl-segment-foreground ring-tl-obscure-border absolute inset-y-0 left-[48%] w-[28%] rounded-md px-2 text-[11px] leading-6 font-medium ring-1">
            Blur
          </div>
        </div>
        <div className="bg-playhead absolute top-1 bottom-1 left-[62%] w-px" />
      </div>
      <div className="bg-stage flex h-20 items-center justify-center rounded-md">
        <div
          className="relative h-12 w-40 rounded-sm"
          style={{ background: "linear-gradient(135deg, #64748b, #cbd5e1)" }}
        >
          <div className="border-brand absolute -inset-1.5 border border-dashed" />
          {[
            "-top-2.5 -left-2.5",
            "-top-2.5 -right-2.5",
            "-bottom-2.5 -left-2.5",
            "-right-2.5 -bottom-2.5",
          ].map((at) => (
            <div
              key={at}
              className={`bg-brand-foreground border-brand absolute size-2 border-[1.5px] ${at}`}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

/** Edits a custom theme; `otherNames` are the names it can't take. */
export function ThemeEditor({
  theme,
  otherNames,
  onChange,
}: {
  theme: CustomTheme;
  otherNames: string[];
  onChange: (theme: CustomTheme) => void;
}) {
  const colors = customTheme(theme).colors;
  const original = originalColors(theme);
  const issues = useMemo(
    () => contrastIssues(colors, theme.base),
    [colors, theme.base],
  );
  const [name, setName] = useState<string | null>(null);
  const latest = useRef(theme);
  latest.current = theme;
  const changeColor = useCallback(
    (token: TokenKey, value: string) => {
      const current = latest.current;
      onChange({ ...current, colors: { ...current.colors, [token]: value } });
    },
    [onChange],
  );
  const edited = TOKENS.some((t) => colors[t.key] !== original[t.key]);

  return (
    <section aria-label="Theme editor" className="space-y-4">
      <div className="flex flex-wrap items-end gap-3">
        <div className="min-w-40 flex-1 space-y-1.5">
          <Label htmlFor="theme-name">Name</Label>
          <Input
            id="theme-name"
            maxLength={MAX_NAME_LENGTH}
            value={name ?? theme.name}
            onChange={(e) => {
              setName(e.target.value);
              const trimmed = clampName(e.target.value.trim());
              if (trimmed) onChange({ ...theme, name: trimmed });
            }}
            onBlur={() => {
              setName(null);
              const unique = uniqueName(theme.name, otherNames);
              if (unique !== theme.name) onChange({ ...theme, name: unique });
            }}
          />
        </div>
        <ToggleGroup
          aria-label="Base"
          type="single"
          variant="outline"
          size="sm"
          value={theme.base}
          onValueChange={(base) =>
            base && onChange({ ...theme, base: base as ThemeBase })
          }
        >
          <ToggleGroupItem value="light" className="px-3">
            Light
          </ToggleGroupItem>
          <ToggleGroupItem value="dark" className="px-3">
            Dark
          </ToggleGroupItem>
        </ToggleGroup>
        <Button
          size="sm"
          variant="outline"
          disabled={!edited}
          onClick={() => onChange({ ...theme, colors: { ...original } })}
        >
          <RotateCcw /> Reset All
        </Button>
      </div>
      <ThemePreview />
      {issues.length > 0 && (
        <p role="status" className="text-warning text-xs">
          Hard to read:{" "}
          {issues
            .map((i) => `${i.label} ${i.ratio.toFixed(1)}:1 (needs ${i.min}:1)`)
            .join(", ")}
          .
        </p>
      )}
      {TOKEN_GROUPS.map((group) => (
        <fieldset key={group} className="space-y-1.5">
          <legend className="text-muted-foreground mb-1.5 text-xs font-medium tracking-wide uppercase">
            {group}
          </legend>
          <div className="grid grid-cols-1 gap-x-5 gap-y-1.5 sm:grid-cols-2">
            {TOKENS.filter((t) => t.group === group).map((t) => (
              <TokenRow
                key={t.key}
                token={t.key}
                label={t.label}
                value={colors[t.key]}
                original={original[t.key]}
                onChange={changeColor}
              />
            ))}
          </div>
        </fieldset>
      ))}
      <p className="text-muted-foreground text-xs">
        Colors take #rrggbb, or #rrggbbaa for see-through ones.
      </p>
    </section>
  );
}
