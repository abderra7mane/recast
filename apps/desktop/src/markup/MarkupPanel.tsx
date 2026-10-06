import { Button } from "@/components/ui/button";
import { BackgroundControls } from "@/editor/inspector/BackgroundControls";
import {
  ColorField,
  Field,
  Hint,
  Section,
  SliderField,
  SwitchField,
} from "@/editor/inspector/fields";
import { cropArea } from "@/markup/crop";
import { styleOptions, type Style } from "@/markup/model";
import { markupStore, useMarkup } from "@/markup/store";

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

function StyleSection() {
  const tool = useMarkup((s) => s.tool);
  const style = useMarkup((s) => s.style);
  const selectedKind = useMarkup(
    (s) => s.doc.shapes.find((shape) => shape.id === s.selected)?.kind,
  );
  const kind = selectedKind ?? tool;
  const options = styleOptions(kind);
  const set = (patch: Partial<Style>) => markupStore.getState().setStyle(patch);
  const highlight = kind === "highlight";
  const color = highlight ? style.highlight : style.color;
  const setColor = (value: string) =>
    set(highlight ? { highlight: value } : { color: value });

  if (kind === "blur" || kind === "pixelate") {
    return (
      <Section title="Style">
        <Hint>
          Blur and pixelate hide the screenshot under them: the saved image
          keeps no detail there. Marks stay on top.
        </Hint>
      </Section>
    );
  }
  if (!options.color) {
    return (
      <Section title="Style">
        <Hint>Select a mark to change its style, or pick a tool.</Hint>
      </Section>
    );
  }
  return (
    <Section title="Style">
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
          onChange={(width) => set({ width })}
        />
      )}
      {options.fill && (
        <SwitchField
          label="Fill"
          hint="Fills the shape with its color."
          checked={style.fill}
          onChange={(fill) => set({ fill })}
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
          onChange={(fontSize) => set({ fontSize })}
        />
      )}
    </Section>
  );
}

function CropSection() {
  const crop = useMarkup((s) => s.doc.crop);
  const image = useMarkup((s) => s.image);
  const area = cropArea(crop, image);
  return (
    <Section title="Crop">
      <Field
        label="Size"
        hint="Only this part is kept. The original stays until you click Done."
      >
        <span className="text-muted-foreground font-mono text-xs tabular-nums">
          {area.width} × {area.height}
        </span>
      </Field>
      <div className="flex gap-2">
        <Button
          size="sm"
          variant="outline"
          disabled={crop === null}
          onClick={() => markupStore.getState().setCrop(null)}
        >
          Reset
        </Button>
        <Button
          size="sm"
          className="ml-auto"
          onClick={() => markupStore.getState().setTool("select")}
        >
          Apply
        </Button>
      </div>
    </Section>
  );
}

function BeautifySection() {
  const beautify = useMarkup((s) => s.doc.beautify);
  const background = useMarkup((s) => s.doc.background);
  return (
    <>
      <Section title="Beautify">
        <SwitchField
          label="Beautify"
          hint="Puts the screenshot on a background with padding, rounded corners and a shadow."
          checked={beautify}
          onChange={(on) => markupStore.getState().setBeautify(on)}
        />
      </Section>
      {beautify && (
        <BackgroundControls
          background={background}
          onChange={(patch) => markupStore.getState().setBackground(patch)}
        />
      )}
    </>
  );
}

/** Style of the selected mark or tool, crop and Beautify. */
export function MarkupPanel() {
  const cropping = useMarkup((s) => s.tool === "crop");
  return (
    <>
      {cropping ? <CropSection /> : <StyleSection />}
      <BeautifySection />
    </>
  );
}
