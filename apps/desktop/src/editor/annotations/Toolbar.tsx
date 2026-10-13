import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { AnnotationTool } from "@/editor/annotations/model";
import { ANNOTATION_TOOLS } from "@/editor/annotations/tools";
import { editorStore, useEditor } from "@/editor/store";
import { Tip } from "@/editor/Tip";

/** Picks what a drag on the video does: select, or draw an annotation. */
export function AnnotationToolbar() {
  const tool = useEditor((s) => s.tool);
  return (
    <ToggleGroup
      type="single"
      size="sm"
      aria-label="Annotation tools"
      value={tool}
      onValueChange={(value) =>
        value && editorStore.getState().setTool(value as AnnotationTool)
      }
    >
      {ANNOTATION_TOOLS.map(({ tool, label, key, icon: Icon }) => (
        <Tip key={tool} label={`${label} (${key.toUpperCase()})`}>
          <ToggleGroupItem value={tool} aria-label={label} className="px-2.5">
            <Icon />
          </ToggleGroupItem>
        </Tip>
      ))}
    </ToggleGroup>
  );
}
