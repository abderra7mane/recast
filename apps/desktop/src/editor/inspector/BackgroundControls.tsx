import { open } from "@tauri-apps/plugin-dialog";

import { Button } from "@/components/ui/button";
import {
  Choice,
  ColorField,
  Section,
  SliderField,
} from "@/editor/inspector/fields";
import { percent, type Settings } from "@/editor/settings";

export type Background = Settings["background"];
type Fill = Background["fill"];

const DEFAULT_FILLS: Record<Fill["kind"], Fill> = {
  solid: { kind: "solid", color: "#1f2937" },
  gradient: { kind: "gradient", from: "#4f46e5", to: "#db2777", angleDeg: 135 },
  image: { kind: "image", path: "" },
};

/** Fill, padding, corner radius and shadow, shared by the editor and Beautify. */
export function BackgroundControls({
  background: bg,
  onChange,
}: {
  background: Background;
  onChange: (patch: Partial<Background>) => void;
}) {
  const fill = bg.fill;

  const pickImage = async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg"] }],
    });
    if (typeof path === "string") onChange({ fill: { kind: "image", path } });
  };

  return (
    <>
      <Section title="Fill">
        <Choice
          value={fill.kind}
          options={[
            { value: "solid", label: "Solid" },
            { value: "gradient", label: "Gradient" },
            { value: "image", label: "Image" },
          ]}
          onChange={(kind) => {
            if (kind === "image") void pickImage();
            else onChange({ fill: DEFAULT_FILLS[kind] });
          }}
        />
        {fill.kind === "solid" && (
          <ColorField
            label="Color"
            value={fill.color}
            onChange={(color) => onChange({ fill: { ...fill, color } })}
          />
        )}
        {fill.kind === "gradient" && (
          <>
            <ColorField
              label="From"
              value={fill.from}
              onChange={(from) => onChange({ fill: { ...fill, from } })}
            />
            <ColorField
              label="To"
              value={fill.to}
              onChange={(to) => onChange({ fill: { ...fill, to } })}
            />
            <SliderField
              label="Angle"
              value={fill.angleDeg}
              min={0}
              max={360}
              step={1}
              format={(v) => `${Math.round(v)}°`}
              onChange={(angleDeg) => onChange({ fill: { ...fill, angleDeg } })}
            />
          </>
        )}
        {fill.kind === "image" && (
          <div className="space-y-2">
            <p
              className="text-muted-foreground truncate text-xs"
              title={fill.path}
            >
              {fill.path ? fill.path.split("/").pop() : "No image chosen"}
            </p>
            <Button
              size="sm"
              variant="outline"
              onClick={() => void pickImage()}
            >
              Choose image…
            </Button>
          </div>
        )}
      </Section>
      <Section title="Frame">
        <SliderField
          label="Padding"
          value={bg.padding}
          min={0}
          max={0.3}
          step={0.005}
          format={percent}
          onChange={(padding) => onChange({ padding })}
        />
        <SliderField
          label="Corner radius"
          value={bg.cornerRadius}
          min={0}
          max={0.1}
          step={0.001}
          format={percent}
          onChange={(cornerRadius) => onChange({ cornerRadius })}
        />
      </Section>
      <Section title="Shadow">
        <SliderField
          label="Opacity"
          value={bg.shadow.opacity}
          min={0}
          max={1}
          format={percent}
          onChange={(opacity) =>
            onChange({ shadow: { ...bg.shadow, opacity } })
          }
        />
        <SliderField
          label="Blur"
          value={bg.shadow.blur}
          min={0}
          max={0.15}
          step={0.002}
          format={percent}
          onChange={(blur) => onChange({ shadow: { ...bg.shadow, blur } })}
        />
        <SliderField
          label="Offset"
          value={bg.shadow.offsetY}
          min={-0.05}
          max={0.05}
          step={0.001}
          format={percent}
          onChange={(offsetY) =>
            onChange({ shadow: { ...bg.shadow, offsetY } })
          }
        />
      </Section>
    </>
  );
}
