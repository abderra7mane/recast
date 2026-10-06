import { useRef, type ReactNode } from "react";
import {
  AudioLines,
  Film,
  ImageIcon,
  MousePointer2,
  MousePointerClick,
  Play,
  Trash2,
  ZoomIn,
} from "lucide-react";

import {
  commands,
  type Project,
  type Resolution,
  type SoundPack,
} from "@/bindings";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
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
  Hint,
  Section,
  SliderField,
  SwitchField,
} from "@/editor/inspector/fields";
import { BackgroundControls } from "@/editor/inspector/BackgroundControls";
import { autoSize, ENLARGE_WARNING, enlargement } from "@/editor/export-size";
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
  hint,
  value,
  options,
  onChange,
  children,
}: {
  label: string;
  hint: ReactNode;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  children?: ReactNode;
}) {
  const stacked = children !== undefined;
  const select = (
    <Select value={value} onValueChange={(v) => onChange(v as T)}>
      <SelectTrigger
        size="sm"
        className={stacked ? "min-w-0 flex-1" : "w-36"}
        aria-label={label}
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent position="popper">
        {options.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
  if (stacked) {
    return (
      <div data-slot="field" className="space-y-1.5">
        <Label className="text-sm font-normal">{label}</Label>
        <div data-slot="field-row" className="flex items-center gap-1">
          {select}
          {children}
        </div>
        <Hint>{hint}</Hint>
      </div>
    );
  }
  return (
    <div data-slot="field" className="space-y-1">
      <div
        data-slot="field-row"
        className="flex items-center justify-between gap-3"
      >
        <Label className="text-sm font-normal">{label}</Label>
        {select}
      </div>
      <Hint>{hint}</Hint>
    </div>
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
        hint="How big Recast draws the cursor."
        value={cursor.size}
        min={0.5}
        max={4}
        step={0.05}
        format={times}
        onChange={(size) => update("cursor", { size })}
      />
      <SliderField
        label="Smoothing"
        hint="Evens out shaky cursor movement. More is smoother but lags behind."
        value={cursor.smoothing}
        min={0}
        max={1}
        format={percent}
        onChange={(smoothing) => update("cursor", { smoothing })}
      />
      <SwitchField
        label="Hide when idle"
        hint="Fades the cursor out while it doesn't move."
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
        hint="How far this segment zooms in."
        value={segment.level}
        min={1}
        max={4}
        step={0.05}
        format={times}
        onChange={(level) => updateSegment(selected, { level })}
      />
      <Choice
        label="Focus"
        hint="Follow cursor pans along with the pointer. Fixed point stays on one spot."
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
            hint="Where the fixed point is, from left to right."
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
            hint="Where the fixed point is, from top to bottom."
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
      <Field
        label="Remove"
        hint="Deletes this zoom. The Delete key does the same."
      >
        <Button
          size="sm"
          variant="outline"
          onClick={() => deleteSegment(selected)}
        >
          <Trash2 /> Delete
        </Button>
      </Field>
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
          hint={
            zoom.auto
              ? "Zooms in where you click and back out when you pause. Editing a segment turns it off and keeps the segments."
              : "Zooms in where you click and back out when you pause."
          }
          checked={zoom.auto}
          onChange={(auto) => update("zoom", { auto })}
        />
        <SliderField
          label="Default zoom"
          hint="How far auto zoom and new segments zoom in."
          value={zoom.level}
          min={1.25}
          max={4}
          step={0.05}
          format={times}
          onChange={(level) => update("zoom", { level })}
        />
      </Section>
      <SegmentEditor />
    </>
  );
}

const PACKS: { value: SoundPack; label: string }[] = [
  { value: "mouseClick", label: "Mouse click" },
  { value: "softTap", label: "Soft tap" },
  { value: "mechanical", label: "Mechanical keyboard" },
  { value: "trackpadTap", label: "Trackpad tap" },
  { value: "pop", label: "Pop" },
  { value: "tick", label: "Subtle tick" },
];

let previewContext: AudioContext | null = null;

async function previewSound(pack: SoundPack, volume: number) {
  const wav = await commands.clickSoundPreview(pack);
  previewContext ??= new AudioContext();
  const context = previewContext;
  const buffer = await context.decodeAudioData(new Uint8Array(wav).buffer);
  const source = context.createBufferSource();
  source.buffer = buffer;
  const gain = context.createGain();
  gain.gain.value = volume;
  source.connect(gain).connect(context.destination);
  source.start();
}

function ClicksTab({ settings }: { settings: Settings }) {
  const { clicks, sounds } = settings;
  return (
    <>
      <Section title="Effects">
        <SwitchField
          label="Ripple"
          hint="Shows a ring where you click."
          checked={clicks.ripple}
          onChange={(ripple) => update("clicks", { ripple })}
        />
        <ColorField
          label="Ripple color"
          hint="The color of the click ring."
          value={clicks.color}
          onChange={(color) => update("clicks", { color })}
        />
        <SliderField
          label="Ripple size"
          hint="How big the click ring grows."
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
          hint="Briefly shrinks the cursor on each click."
          checked={clicks.squish}
          onChange={(squish) => update("clicks", { squish })}
        />
      </Section>
      <Section title="Sounds">
        <SwitchField
          label="Click sounds"
          hint="Plays a click sound in the video for each click."
          checked={sounds.enabled}
          onChange={(enabled) => update("sounds", { enabled })}
        />
        <SelectField
          label="Sound pack"
          hint="The sound each click makes. Press play to hear it."
          value={sounds.pack}
          options={PACKS}
          onChange={(pack) => update("sounds", { pack })}
        >
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                size="icon"
                variant="ghost"
                className="size-8"
                aria-label="Play the sound"
                onClick={() => void previewSound(sounds.pack, sounds.volume)}
              >
                <Play />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Play the sound</TooltipContent>
          </Tooltip>
        </SelectField>
        <SliderField
          label="Volume"
          hint="How loud the click sounds are."
          value={sounds.volume}
          min={0}
          max={1}
          format={percent}
          disabled={!sounds.enabled}
          onChange={(volume) => update("sounds", { volume })}
        />
        <SwitchField
          label="Separate left and right"
          hint="Gives right clicks a sound of their own."
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
  source,
  missing,
  value,
  available,
  onChange,
}: {
  label: string;
  /** What the track holds, as in "how loud <source> is". */
  source: string;
  /** What a recording without the track lacks, as in "has no <missing>". */
  missing: string;
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
        hint={
          available
            ? `Leaves ${source} out of the video.`
            : `This recording has no ${missing}.`
        }
        checked={available && value === 0}
        disabled={!available}
        onChange={(muted) => onChange(muted ? 0 : lastAudible.current)}
      />
      <SliderField
        label={`${label} volume`}
        hint={`How loud ${source} is in the video.`}
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
        source="the microphone"
        missing="microphone audio"
        value={audio.micVolume}
        available={project.recording.mic !== null}
        onChange={(micVolume) => update("audio", { micVolume })}
      />
      <VolumeField
        label="System audio"
        source="the sound your Mac played"
        missing="system audio"
        value={audio.systemVolume}
        available={project.recording.systemAudio !== null}
        onChange={(systemVolume) => update("audio", { systemVolume })}
      />
    </Section>
  );
}

const RESOLUTIONS: { value: Resolution; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "1080p", label: "1080p" },
  { value: "1440p", label: "1440p" },
  { value: "4k", label: "4K" },
];

function ExportTab({
  settings,
  project,
}: {
  settings: Settings;
  project: Project;
}) {
  const exp = settings.export;
  const video = {
    width: project.recording.width,
    height: project.recording.height,
  };
  const padding = settings.background.padding;
  const [autoWidth, autoHeight] = autoSize(video, padding);
  const scale = enlargement(video, padding, exp.resolution);
  const preset = RESOLUTIONS.find((r) => r.value === exp.resolution)?.label;
  return (
    <Section>
      <SelectField
        label="Codec"
        hint="H.264 plays everywhere. HEVC makes smaller files."
        value={exp.codec}
        options={[
          { value: "h264", label: "H.264" },
          { value: "hevc", label: "HEVC (H.265)" },
        ]}
        onChange={(codec) => update("export", { codec })}
      />
      <SelectField
        label="Resolution"
        hint={`Auto keeps every recorded pixel sharp (${autoWidth} × ${autoHeight}). The others resize to a set height.`}
        value={exp.resolution}
        options={RESOLUTIONS}
        onChange={(resolution) => update("export", { resolution })}
      />
      {scale > ENLARGE_WARNING && (
        <p role="status" className="text-xs text-amber-500">
          The recording is {video.width} × {video.height} px. {preset} enlarges
          it {scale.toFixed(1)}×, so text may look soft.
        </p>
      )}
      <SelectField
        label="Frame rate"
        hint="60 fps moves more smoothly. 30 fps makes smaller files."
        value={exp.fps}
        options={[
          { value: "30", label: "30 fps" },
          { value: "60", label: "60 fps" },
        ]}
        onChange={(fps) => update("export", { fps })}
      />
      <SliderField
        label="Quality"
        hint="Higher looks better and makes larger files."
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
            <ExportTab settings={settings} project={project} />
          </TabsContent>
        </div>
      </div>
    </Tabs>
  );
}
