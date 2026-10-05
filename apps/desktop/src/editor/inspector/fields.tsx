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

export function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-3">
      <Label className="text-sm font-normal">{label}</Label>
      {children}
    </div>
  );
}

/** A slider whose drag is one undo step. */
export function SliderField({
  label,
  value,
  min,
  max,
  step = 0.01,
  format = (v) => v.toFixed(2),
  disabled,
  onChange,
}: {
  label: string;
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
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <Label className="text-sm font-normal">{label}</Label>
        <span className="text-muted-foreground font-mono text-xs tabular-nums">
          {format(value)}
        </span>
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
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <Field label={label}>
      <Switch
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
      />
    </Field>
  );
}

export function Choice<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: ReactNode }[];
  onChange: (value: T) => void;
}) {
  return (
    <ToggleGroup
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
  );
}

/**
 * A native color picker for `#rrggbb` or `#rrggbbaa` that keeps the alpha. Changes
 * while the picker is open form one undo step.
 */
export function ColorField({
  label,
  value,
  onChange,
}: {
  label: string;
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
    <Field label={label}>
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
