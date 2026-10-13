import {
  ColorField,
  SliderField,
  SwitchField,
} from "@/editor/inspector/fields";
import { styleOptions, type Style, type Tool } from "@/markup/model";

const SWATCHES = [
  { color: "#ff3b30", name: "Red" },
  { color: "#ff9500", name: "Orange" },
  { color: "#ffd60a", name: "Yellow" },
  { color: "#34c759", name: "Green" },
  { color: "#0a84ff", name: "Blue" },
  { color: "#bf5af2", name: "Purple" },
  { color: "#000000", name: "Black" },
  { color: "#ffffff", name: "White" },
];

function Swatches({
  value,
  onChange,
}: {
  value: string;
  onChange: (color: string) => void;
}) {
  return (
    <div className="flex flex-wrap gap-1.5" role="group" aria-label="Colors">
      {SWATCHES.map(({ color, name }) => (
        <button
          key={color}
          type="button"
          aria-label={name}
          aria-pressed={value.toLowerCase() === color}
          title={name}
          className="size-6 rounded-full border border-white/20 ring-offset-2 ring-offset-black aria-pressed:ring-2 aria-pressed:ring-sky-400"
          style={{ background: color }}
          onClick={() => onChange(color)}
        />
      ))}
    </div>
  );
}

/**
 * The color, line width, fill and font size of marks of `kind`, as far as they apply;
 * nothing for kinds without a color.
 */
export function StyleFields({
  kind,
  style,
  onChange,
}: {
  kind: Tool;
  style: Style;
  onChange: (patch: Partial<Style>) => void;
}) {
  const options = styleOptions(kind);
  if (!options.color) return null;
  const highlight = kind === "highlight";
  const color = highlight ? style.highlight : style.color;
  const setColor = (value: string) =>
    onChange(highlight ? { highlight: value } : { color: value });
  return (
    <>
      <Swatches value={color} onChange={setColor} />
      <ColorField
        label="Color"
        hint={
          highlight
            ? "The marker color; it stays see-through."
            : "Any other color."
        }
        value={color}
        onChange={setColor}
      />
      {options.width && (
        <SliderField
          label="Width"
          hint="How thick the line is, in points."
          value={style.width}
          min={1}
          max={20}
          step={0.5}
          format={(v) => `${v} pt`}
          onChange={(width) => onChange({ width })}
        />
      )}
      {options.fill && (
        <SwitchField
          label="Fill"
          hint="Fills the shape with its color."
          checked={style.fill}
          onChange={(fill) => onChange({ fill })}
        />
      )}
      {options.fontSize && (
        <SliderField
          label="Font size"
          hint="Text size in points."
          value={style.fontSize}
          min={8}
          max={120}
          step={1}
          format={(v) => `${Math.round(v)} pt`}
          onChange={(fontSize) => onChange({ fontSize })}
        />
      )}
    </>
  );
}
