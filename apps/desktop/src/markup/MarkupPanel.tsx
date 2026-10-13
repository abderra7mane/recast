import { Button } from "@/components/ui/button";
import { BackgroundControls } from "@/editor/inspector/BackgroundControls";
import { Field, Hint, Section, SwitchField } from "@/editor/inspector/fields";
import { cropArea } from "@/markup/crop";
import { styleOptions } from "@/markup/model";
import { markupStore, useMarkup } from "@/markup/store";
import { StyleFields } from "@/markup/StyleFields";

function StyleSection() {
  const tool = useMarkup((s) => s.tool);
  const style = useMarkup((s) => s.style);
  const selectedKind = useMarkup(
    (s) => s.doc.shapes.find((shape) => shape.id === s.selected)?.kind,
  );
  const kind = selectedKind ?? tool;

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
  if (!styleOptions(kind).color) {
    return (
      <Section title="Style">
        <Hint>Select a mark to change its style, or pick a tool.</Hint>
      </Section>
    );
  }
  return (
    <Section title="Style">
      <StyleFields
        kind={kind}
        style={style}
        onChange={(patch) => markupStore.getState().setStyle(patch)}
      />
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
