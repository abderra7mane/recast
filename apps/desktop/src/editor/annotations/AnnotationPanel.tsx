import { useContext, useEffect, useState } from "react";
import { Plus, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  isObscure,
  MAX_BLOCK,
  MAX_TRACKS,
  MIN_BLOCK,
  moveAnnotation,
  moveToTrack,
  resizeAnnotation,
  type AnnotationStyle,
  type AnnotationTool,
} from "@/editor/annotations/model";
import { annotationHint, annotationLabel } from "@/editor/annotations/tools";
import { Field, Hint, Section, SliderField } from "@/editor/inspector/fields";
import { GestureContext } from "@/editor/inspector/gesture";
import type { Annotation } from "@/editor/settings";
import { annotationById, editorStore, useEditor } from "@/editor/store";
import { StyleFields } from "@/markup/StyleFields";

const setStyle = (patch: Partial<AnnotationStyle>) =>
  editorStore.getState().setStyle(patch);

function BlockField({ value }: { value: number }) {
  return (
    <SliderField
      label="Cell size"
      hint="How large the areas averaged together are. Larger hides more."
      value={value}
      min={MIN_BLOCK}
      max={MAX_BLOCK}
      step={1}
      format={(v) => `${Math.round(v)} pt`}
      onChange={(block) => setStyle({ block })}
    />
  );
}

function StyleOf({
  kind,
  style,
}: {
  kind: AnnotationTool;
  style: AnnotationStyle;
}) {
  if (kind === "blur" || kind === "pixelate")
    return <BlockField value={style.block} />;
  return <StyleFields kind={kind} style={style} onChange={setStyle} />;
}

/** Seconds, applied when the field loses focus or Return is pressed. */
function SecondsField({
  label,
  hint,
  value,
  onChange,
}: {
  label: string;
  hint: string;
  value: number;
  onChange: (ms: number) => void;
}) {
  const shown = (value / 1000).toFixed(2);
  const [text, setText] = useState(shown);
  useEffect(() => setText(shown), [shown]);
  const apply = () => {
    const seconds = Number.parseFloat(text);
    if (Number.isFinite(seconds)) onChange(seconds * 1000);
    else setText(shown);
  };
  return (
    <Field label={label} hint={hint}>
      <Input
        type="number"
        aria-label={label}
        className="h-8 w-24 text-right font-mono text-xs tabular-nums"
        step={0.1}
        min={0}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={apply}
        onKeyDown={(e) => {
          if (e.key === "Enter") apply();
        }}
      />
    </Field>
  );
}

function TextField({ annotation }: { annotation: Annotation }) {
  const { begin, end } = useContext(GestureContext);
  if (annotation.shape.kind !== "text") return null;
  const shape = annotation.shape;
  return (
    <div data-slot="field" className="space-y-1.5">
      <Label className="text-sm font-normal">Text</Label>
      <textarea
        aria-label="Text"
        rows={2}
        className="border-input dark:bg-input/30 w-full resize-y rounded-md border bg-transparent px-3 py-1.5 text-sm"
        value={shape.text}
        onFocus={begin}
        onBlur={end}
        onChange={(e) =>
          editorStore.getState().updateAnnotation(annotation.id, {
            shape: { ...shape, text: e.target.value },
          })
        }
      />
      <Hint>What the annotation says; Return starts a new line.</Hint>
    </div>
  );
}

function LayerField({ annotation }: { annotation: Annotation }) {
  const tracks = useEditor((s) => s.settings?.annotations.tracks ?? 1);
  const items = useEditor((s) => s.settings?.annotations.items ?? []);
  return (
    <div data-slot="field" className="space-y-1">
      <div
        data-slot="field-row"
        className="flex items-center justify-between gap-3"
      >
        <Label className="text-sm font-normal">Layer</Label>
        <Select
          value={String(annotation.track)}
          onValueChange={(value) => {
            const moved = moveToTrack(items, annotation.id, Number(value));
            if (moved) editorStore.getState().setAnnotationItems(moved);
          }}
        >
          <SelectTrigger size="sm" className="w-36" aria-label="Layer">
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            {Array.from({ length: tracks }, (_, track) => (
              <SelectItem
                key={track}
                value={String(track)}
                disabled={
                  track !== annotation.track &&
                  !moveToTrack(items, annotation.id, track)
                }
              >
                Layer {track + 1}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <Hint>Lower layers on the timeline are drawn on top.</Hint>
    </div>
  );
}

function SelectedAnnotation({
  annotation,
  style,
}: {
  annotation: Annotation;
  style: AnnotationStyle;
}) {
  const durationMs = useEditor((s) => s.durationMs);
  const kind = annotation.shape.kind;
  const store = editorStore.getState();
  const items = () => editorStore.getState().settings?.annotations.items ?? [];
  return (
    <>
      <Section title={annotationLabel(kind)}>
        {isObscure(annotation.shape) && (
          <Hint>
            Hides the recording under it in every frame it shows, following the
            zoom, and the annotations on layers above it on the timeline.
          </Hint>
        )}
        <TextField annotation={annotation} />
        <StyleOf kind={kind} style={style} />
      </Section>
      <Section title="Timing">
        <SecondsField
          label="Start"
          hint="When it appears, in seconds of the recording."
          value={annotation.startMs}
          onChange={(ms) =>
            store.setAnnotationItems(
              moveAnnotation(
                items(),
                annotation.id,
                ms - annotation.startMs,
                annotation.track,
                durationMs,
              ),
            )
          }
        />
        <SecondsField
          label="Duration"
          hint="How long it shows, in seconds."
          value={annotation.endMs - annotation.startMs}
          onChange={(ms) =>
            store.setAnnotationItems(
              resizeAnnotation(
                items(),
                annotation.id,
                "end",
                annotation.startMs + ms,
                durationMs,
              ),
            )
          }
        />
        <LayerField annotation={annotation} />
      </Section>
      <Section>
        <Field
          label="Remove"
          hint="Deletes this annotation. The Delete key does the same."
        >
          <Button
            size="sm"
            variant="outline"
            onClick={() => store.deleteAnnotation(annotation.id)}
          >
            <Trash2 /> Delete
          </Button>
        </Field>
      </Section>
    </>
  );
}

function NewAnnotations({
  tool,
  style,
}: {
  tool: AnnotationTool;
  style: AnnotationStyle;
}) {
  if (tool === "select") {
    return (
      <Section title="Annotations">
        <Hint>
          Pick a tool above the video and drag on it to add an annotation at the
          playhead, or select one on the video or the timeline to change it.
        </Hint>
      </Section>
    );
  }
  return (
    <Section title={`New ${annotationLabel(tool).toLowerCase()}`}>
      <Hint>{annotationHint(tool)}</Hint>
      <StyleOf kind={tool} style={style} />
    </Section>
  );
}

function Layers() {
  const tracks = useEditor((s) => s.settings?.annotations.tracks ?? 1);
  const counts = useEditor((s) => {
    const items = s.settings?.annotations.items ?? [];
    return Array.from(
      { length: tracks },
      (_, track) => items.filter((a) => a.track === track).length,
    ).join(" ");
  });
  const perTrack = counts.split(" ").map(Number);
  const { addTrack, deleteTrack } = editorStore.getState();
  return (
    <Section title="Layers">
      {perTrack.map((count, track) => (
        <Field
          key={track}
          label={`Layer ${track + 1}`}
          hint={`${count} annotation${count === 1 ? "" : "s"}`}
        >
          <Button
            size="icon"
            variant="ghost"
            className="size-8"
            aria-label={`Delete layer ${track + 1}`}
            title="Delete the layer and its annotations"
            onClick={() => deleteTrack(track)}
          >
            <Trash2 />
          </Button>
        </Field>
      ))}
      <Field
        label="Add a layer"
        hint="A new layer goes below the others on the timeline and is drawn on top."
      >
        <Button
          size="sm"
          variant="outline"
          disabled={tracks >= MAX_TRACKS}
          onClick={addTrack}
        >
          <Plus /> Add
        </Button>
      </Field>
    </Section>
  );
}

/** The selected annotation, or the style of new ones, and the annotation layers. */
export function AnnotationPanel() {
  const annotation = useEditor((s) => annotationById(s, s.selectedAnnotation));
  const tool = useEditor((s) => s.tool);
  const style = useEditor((s) => s.style);
  return (
    <>
      {annotation ? (
        <SelectedAnnotation annotation={annotation} style={style} />
      ) : (
        <NewAnnotations tool={tool} style={style} />
      )}
      <Layers />
    </>
  );
}
