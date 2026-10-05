import { useRef } from "react";
import {
  AudioLines,
  Film,
  ImageIcon,
  MousePointer2,
  MousePointerClick,
  Trash2,
  ZoomIn,
} from "lucide-react";

import type { Project, SoundPack } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  Choice,
  ColorField,
  Field,
  Section,
  SliderField,
  SwitchField,
} from "@/editor/inspector/fields";
import { BackgroundControls } from "@/editor/inspector/BackgroundControls";
import {
  percent,
  times,
  type Section as SettingsSection,
  type Settings,
} from "@/editor/settings";
import { useEditor, visibleSegments } from "@/editor/store";
import { editorStore } from "@/editor/store";

export type InspectorTab =
  "background" | "cursor" | "zoom" | "clicks" | "audio" | "export";

const TABS: { id: InspectorTab; label: string; icon: typeof Film }[] = [
  { id: "background", label: "Background", icon: ImageIcon },
  { id: "cursor", label: "Cursor", icon: MousePointer2 },
  { id: "zoom", label: "Zoom", icon: ZoomIn },
  { id: "clicks", label: "Clicks & Sounds", icon: MousePointerClick },
  { id: "audio", label: "Audio", icon: AudioLines },
  { id: "export", label: "Export", icon: Film },
];

function update<K extends SettingsSection>(
  section: K,
  patch: Partial<Settings[K]>,
) {
  editorStore.getState().update(section, patch);
}

function SelectField<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <Field label={label}>
      <Select value={value} onValueChange={(v) => onChange(v as T)}>
        <SelectTrigger size="sm" className="w-36">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map((o) => (
            <SelectItem key={o.value} value={o.value}>
              {o.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </Field>
  );
}

function BackgroundTab({ settings }: { settings: Settings }) {
  return (
    <BackgroundControls
      background={settings.background}
      onChange={(patch) => update("background", patch)}
    />
  );
}

function CursorTab({ settings }: { settings: Settings }) {
  const cursor = settings.cursor;
  return (
    <Section>
      <SliderField
        label="Size"
        value={cursor.size}
        min={0.5}
        max={4}
        step={0.05}
        format={times}
        onChange={(size) => update("cursor", { size })}
      />
      <SliderField
        label="Smoothing"
        value={cursor.smoothing}
        min={0}
        max={1}
        format={percent}
        onChange={(smoothing) => update("cursor", { smoothing })}
      />
      <SwitchField
        label="Hide when idle"
        checked={cursor.hideWhenIdle}
        onChange={(hideWhenIdle) => update("cursor", { hideWhenIdle })}
      />
    </Section>
  );
}

function SegmentEditor() {
  const selected = useEditor((s) => s.selected);
  const segment = useEditor((s) =>
    s.selected === null ? undefined : visibleSegments(s)[s.selected],
  );
  if (selected === null || !segment) {
    return (
      <Section title="Segment">
        <p className="text-muted-foreground text-sm">
          Select a segment on the timeline to change it, or double-click an
          empty spot on the zoom track to add one.
        </p>
      </Section>
    );
  }
  const { updateSegment, deleteSegment } = editorStore.getState();
  const focus = segment.focus;
  return (
    <Section title="Segment">
      <SliderField
        label="Zoom"
        value={segment.level}
        min={1}
        max={4}
        step={0.05}
        format={times}
        onChange={(level) => updateSegment(selected, { level })}
      />
      <Choice
        value={focus.kind}
        options={[
          { value: "followCursor", label: "Follow cursor" },
          { value: "point", label: "Fixed point" },
        ]}
        onChange={(kind) =>
          updateSegment(selected, {
            focus: kind === "point" ? { kind, x: 0.5, y: 0.5 } : { kind },
          })
        }
      />
      {focus.kind === "point" && (
        <>
          <SliderField
            label="Horizontal"
            value={focus.x}
            min={0}
            max={1}
            format={percent}
            onChange={(x) =>
              updateSegment(selected, { focus: { ...focus, x } })
            }
          />
          <SliderField
            label="Vertical"
            value={focus.y}
            min={0}
            max={1}
            format={percent}
            onChange={(y) =>
              updateSegment(selected, { focus: { ...focus, y } })
            }
          />
        </>
      )}
      <Button
        size="sm"
        variant="outline"
        onClick={() => deleteSegment(selected)}
      >
        <Trash2 /> Delete segment
      </Button>
    </Section>
  );
}

function ZoomTab({ settings }: { settings: Settings }) {
  const zoom = settings.zoom;
  return (
    <>
      <Section>
        <SwitchField
          label="Auto zoom on clicks"
          checked={zoom.auto}
          onChange={(auto) => update("zoom", { auto })}
        />
        <SliderField
          label="Default zoom"
          value={zoom.level}
          min={1.25}
          max={4}
          step={0.05}
          format={times}
          onChange={(level) => update("zoom", { level })}
        />
        {zoom.auto && (
          <p className="text-muted-foreground text-xs">
            Editing a segment turns auto zoom off and keeps the segments as they
            are.
          </p>
        )}
      </Section>
      <SegmentEditor />
    </>
  );
}

const PACKS: { value: SoundPack; label: string }[] = [
  { value: "mouseClick", label: "Mouse click" },
  { value: "softTap", label: "Soft tap" },
  { value: "mechanical", label: "Mechanical" },
];

function ClicksTab({ settings }: { settings: Settings }) {
  const { clicks, sounds } = settings;
  return (
    <>
      <Section title="Effects">
        <SwitchField
          label="Ripple"
          checked={clicks.ripple}
          onChange={(ripple) => update("clicks", { ripple })}
        />
        <ColorField
          label="Ripple color"
          value={clicks.color}
          onChange={(color) => update("clicks", { color })}
        />
        <SliderField
          label="Ripple size"
          value={clicks.size}
          min={4}
          max={80}
          step={1}
          format={(v) => `${Math.round(v)} pt`}
          disabled={!clicks.ripple}
          onChange={(size) => update("clicks", { size })}
        />
        <SwitchField
          label="Squish cursor"
          checked={clicks.squish}
          onChange={(squish) => update("clicks", { squish })}
        />
      </Section>
      <Section title="Sounds">
        <SwitchField
          label="Click sounds"
          checked={sounds.enabled}
          onChange={(enabled) => update("sounds", { enabled })}
        />
        <SelectField
          label="Sound pack"
          value={sounds.pack}
          options={PACKS}
          onChange={(pack) => update("sounds", { pack })}
        />
        <SliderField
          label="Volume"
          value={sounds.volume}
          min={0}
          max={1}
          format={percent}
          disabled={!sounds.enabled}
          onChange={(volume) => update("sounds", { volume })}
        />
        <SwitchField
          label="Separate left and right"
          checked={sounds.separateLeftRight}
          onChange={(separateLeftRight) =>
            update("sounds", { separateLeftRight })
          }
        />
      </Section>
    </>
  );
}

function VolumeField({
  label,
  value,
  available,
  onChange,
}: {
  label: string;
  value: number;
  available: boolean;
  onChange: (value: number) => void;
}) {
  const lastAudible = useRef(value > 0 ? value : 1);
  if (value > 0) lastAudible.current = value;
  return (
    <div className="space-y-3">
      <SwitchField
        label={available ? `${label} muted` : `${label} (not recorded)`}
        checked={available && value === 0}
        disabled={!available}
        onChange={(muted) => onChange(muted ? 0 : lastAudible.current)}
      />
      <SliderField
        label={`${label} volume`}
        value={value}
        min={0}
        max={2}
        format={percent}
        disabled={!available}
        onChange={onChange}
      />
    </div>
  );
}

function AudioTab({
  settings,
  project,
}: {
  settings: Settings;
  project: Project;
}) {
  const audio = settings.audio;
  return (
    <Section>
      <VolumeField
        label="Microphone"
        value={audio.micVolume}
        available={project.recording.mic !== null}
        onChange={(micVolume) => update("audio", { micVolume })}
      />
      <VolumeField
        label="System audio"
        value={audio.systemVolume}
        available={project.recording.systemAudio !== null}
        onChange={(systemVolume) => update("audio", { systemVolume })}
      />
    </Section>
  );
}

function ExportTab({ settings }: { settings: Settings }) {
  const exp = settings.export;
  return (
    <Section>
      <SelectField
        label="Codec"
        value={exp.codec}
        options={[
          { value: "h264", label: "H.264" },
          { value: "hevc", label: "HEVC (H.265)" },
        ]}
        onChange={(codec) => update("export", { codec })}
      />
      <SelectField
        label="Resolution"
        value={exp.resolution}
        options={[
          { value: "1080p", label: "1080p" },
          { value: "1440p", label: "1440p" },
          { value: "4k", label: "4K" },
        ]}
        onChange={(resolution) => update("export", { resolution })}
      />
      <SelectField
        label="Frame rate"
        value={exp.fps}
        options={[
          { value: "30", label: "30 fps" },
          { value: "60", label: "60 fps" },
        ]}
        onChange={(fps) => update("export", { fps })}
      />
      <SliderField
        label="Quality"
        value={exp.quality}
        min={0}
        max={1}
        format={percent}
        onChange={(quality) => update("export", { quality })}
      />
    </Section>
  );
}

export function Inspector({
  project,
  tab,
  onTabChange,
}: {
  project: Project;
  tab: InspectorTab;
  onTabChange: (tab: InspectorTab) => void;
}) {
  const settings = useEditor((s) => s.settings);
  if (!settings) return null;
  const current = TABS.find((t) => t.id === tab) ?? TABS[0];
  return (
    <Tabs
      value={tab}
      onValueChange={(v) => onTabChange(v as InspectorTab)}
      orientation="vertical"
      className="flex h-full flex-row-reverse gap-0"
    >
      <TabsList
        variant="line"
        className="h-full justify-start gap-1 border-l px-1.5 py-3"
      >
        {TABS.map(({ id, label, icon: Icon }) => (
          <Tooltip key={id}>
            <TooltipTrigger asChild>
              <TabsTrigger
                value={id}
                aria-label={label}
                className="size-9 flex-none p-0"
              >
                <Icon />
              </TabsTrigger>
            </TooltipTrigger>
            <TooltipContent side="left">{label}</TooltipContent>
          </Tooltip>
        ))}
      </TabsList>
      <div className="flex min-w-0 flex-1 flex-col">
        <h2 className="border-b px-4 py-3 text-sm font-semibold">
          {current.label}
        </h2>
        <div className="min-h-0 flex-1 overflow-y-auto">
          <TabsContent value="background">
            <BackgroundTab settings={settings} />
          </TabsContent>
          <TabsContent value="cursor">
            <CursorTab settings={settings} />
          </TabsContent>
          <TabsContent value="zoom">
            <ZoomTab settings={settings} />
          </TabsContent>
          <TabsContent value="clicks">
            <ClicksTab settings={settings} />
          </TabsContent>
          <TabsContent value="audio">
            <AudioTab settings={settings} project={project} />
          </TabsContent>
          <TabsContent value="export">
            <ExportTab settings={settings} />
          </TabsContent>
        </div>
      </div>
    </Tabs>
  );
}
