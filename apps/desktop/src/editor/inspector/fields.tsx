import { useContext, useEffect, useRef, type ReactNode } from "react";

import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { GestureContext } from "@/editor/inspector/gesture";

export function Section({
  title,
  children,
}: {
  title?: string;
  children: ReactNode;
}) {
  return (
    <section className="space-y-4 border-b px-4 py-4 last:border-b-0">
      {title && (
        <h3 className="text-muted-foreground text-xs font-medium tracking-wide uppercase">
          {title}
        </h3>
      )}
      {children}
    </section>
  );
}

/** The short explanation under an option's label. */
export function Hint({ children }: { children: ReactNode }) {
  return (
    <p data-slot="hint" className="text-muted-foreground text-xs">
      {children}
    </p>
  );
}

/** An option with its label and hint on the left and its control on the right. */
export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint: ReactNode;
  children: ReactNode;
}) {
  return (
    <div data-slot="field" className="flex items-start justify-between gap-3">
      <div className="min-w-0 space-y-0.5 pt-1">
        <Label className="text-sm font-normal">{label}</Label>
        <Hint>{hint}</Hint>
      </div>
      <div className="flex shrink-0 items-center gap-1">{children}</div>
    </div>
  );
}

/** A slider whose drag is one undo step. */
export function SliderField({
  label,
  hint,
  value,
  min,
  max,
  step = 0.01,
  format = (v) => v.toFixed(2),
  disabled,
  onChange,
}: {
  label: string;
  hint: ReactNode;
  value: number;
  min: number;
  max: number;
  step?: number;
  format?: (value: number) => string;
  disabled?: boolean;
  onChange: (value: number) => void;
}) {
  const { begin, end } = useContext(GestureContext);
  return (
    <div data-slot="field" className="space-y-2">
      <div className="space-y-0.5">
        <div className="flex items-center justify-between">
          <Label className="text-sm font-normal">{label}</Label>
          <span className="text-muted-foreground font-mono text-xs tabular-nums">
            {format(value)}
          </span>
        </div>
        <Hint>{hint}</Hint>
      </div>
      <Slider
        value={[value]}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onPointerDown={begin}
        onPointerUp={end}
        onLostPointerCapture={end}
        onValueChange={([v]) => onChange(v)}
        onValueCommit={end}
      />
    </div>
  );
}

export function SwitchField({
  label,
  hint,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  hint: ReactNode;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <Field label={label} hint={hint}>
      <Switch
        aria-label={label}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
      />
    </Field>
  );
}

export function Choice<T extends string>({
  label,
  hint,
  value,
  options,
  onChange,
}: {
  label: string;
  hint: ReactNode;
  value: T;
  options: { value: T; label: ReactNode }[];
  onChange: (value: T) => void;
}) {
  return (
    <div data-slot="field" className="space-y-2">
      <div className="space-y-0.5">
        <Label className="text-sm font-normal">{label}</Label>
        <Hint>{hint}</Hint>
      </div>
      <ToggleGroup
        aria-label={label}
        type="single"
        variant="outline"
        size="sm"
        className="w-full"
        value={value}
        onValueChange={(v) => v && onChange(v as T)}
      >
        {options.map((o) => (
          <ToggleGroupItem key={o.value} value={o.value} className="flex-1">
            {o.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </div>
  );
}

/**
 * A native color picker for `#rrggbb` or `#rrggbbaa` that keeps the alpha. Changes
 * while the picker is open form one undo step.
 */
export function ColorField({
  label,
  hint,
  value,
  onChange,
}: {
  label: string;
  hint: ReactNode;
  value: string;
  onChange: (value: string) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const alpha = value.length === 9 ? value.slice(7) : "";
  const { begin, end } = useContext(GestureContext);

  useEffect(() => {
    const element = input.current;
    if (!element) return;
    element.addEventListener("change", end);
    element.addEventListener("blur", end);
    return () => {
      element.removeEventListener("change", end);
      element.removeEventListener("blur", end);
    };
  }, [end]);

  return (
    <Field label={label} hint={hint}>
      <input
        ref={input}
        type="color"
        aria-label={label}
        className="border-input h-7 w-12 cursor-pointer rounded border bg-transparent p-0.5"
        value={value.slice(0, 7)}
        onChange={(e) => {
          begin();
          onChange(e.target.value + alpha);
        }}
      />
    </Field>
  );
}
