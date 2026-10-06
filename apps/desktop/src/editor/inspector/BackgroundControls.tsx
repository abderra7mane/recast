import { open } from "@tauri-apps/plugin-dialog";

import { Button } from "@/components/ui/button";
import {
  Choice,
  ColorField,
  Field,
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
          label="Fill"
          hint="What fills the space around the picture."
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
            hint="The background color."
            value={fill.color}
            onChange={(color) => onChange({ fill: { ...fill, color } })}
          />
        )}
        {fill.kind === "gradient" && (
          <>
            <ColorField
              label="From"
              hint="The color the gradient starts with."
              value={fill.from}
              onChange={(from) => onChange({ fill: { ...fill, from } })}
            />
            <ColorField
              label="To"
              hint="The color the gradient ends with."
              value={fill.to}
              onChange={(to) => onChange({ fill: { ...fill, to } })}
            />
            <SliderField
              label="Angle"
              hint="The direction the gradient runs in."
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
          <Field
            label="Image"
            hint={
              <span className="block truncate" title={fill.path}>
                {fill.path
                  ? fill.path.split("/").pop()
                  : "A PNG or JPEG that covers the frame."}
              </span>
            }
          >
            <Button
              size="sm"
              variant="outline"
              onClick={() => void pickImage()}
            >
              Choose…
            </Button>
          </Field>
        )}
      </Section>
      <Section title="Frame">
        <SliderField
          label="Padding"
          hint="Space around the picture, as a share of its shorter side."
          value={bg.padding}
          min={0}
          max={0.3}
          step={0.005}
          format={percent}
          onChange={(padding) => onChange({ padding })}
        />
        <SliderField
          label="Corner radius"
          hint="Rounds the picture's corners."
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
          hint="How dark the shadow under the picture is."
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
          hint="How soft the shadow's edge is."
          value={bg.shadow.blur}
          min={0}
          max={0.15}
          step={0.002}
          format={percent}
          onChange={(blur) => onChange({ shadow: { ...bg.shadow, blur } })}
        />
        <SliderField
          label="Offset"
          hint="Moves the shadow down, or up below zero."
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
