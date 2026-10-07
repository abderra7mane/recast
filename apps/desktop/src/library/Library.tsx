import { useCallback, useEffect, useState } from "react";
import {
  AppWindow,
  Crop,
  FolderOpen,
  Monitor,
  Pencil,
  Settings,
  Square,
} from "lucide-react";

import {
  commands,
  events,
  type FinishedRecording,
  type Phase,
  type PickMode,
  type ProjectSummary,
  type ScreenshotSummary,
  type UnfinishedBundle,
} from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { formatElapsed } from "@/recording";

function UnfinishedRow({
  bundle,
  busy,
  onRecover,
  onDiscard,
}: {
  bundle: UnfinishedBundle;
  busy: boolean;
  onRecover: () => void;
  onDiscard: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-sm" title={bundle.path}>
          {bundle.name}
        </span>
        <div className="flex shrink-0 gap-2">
          {confirming ? (
            <>
              <Button
                size="sm"
                variant="destructive"
                disabled={busy}
                onClick={onDiscard}
              >
                Move to Trash
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={busy}
                onClick={() => setConfirming(false)}
              >
                Cancel
              </Button>
            </>
          ) : (
            <>
              {bundle.problem === null && (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={busy}
                  onClick={onRecover}
                >
                  Recover
                </Button>
              )}
              <Button
                size="sm"
                variant="ghost"
                disabled={busy}
                onClick={() => setConfirming(true)}
              >
                Discard
              </Button>
            </>
          )}
        </div>
      </div>
      {bundle.problem && (
        <p className="text-muted-foreground text-xs">
          Can’t be recovered: {bundle.problem}
        </p>
      )}
    </div>
  );
}

function UnfinishedCard({
  bundles,
  onRecovered,
  onDiscarded,
}: {
  bundles: UnfinishedBundle[];
  onRecovered: (result: FinishedRecording) => void;
  onDiscarded: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (bundles.length === 0) return null;

  const run = async <T,>(
    action: () => Promise<
      { status: "ok"; data: T } | { status: "error"; error: string }
    >,
    done: (data: T) => void,
  ) => {
    setBusy(true);
    setError(null);
    const result = await action();
    setBusy(false);
    if (result.status === "ok") done(result.data);
    else setError(result.error);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Unfinished recordings</CardTitle>
        <CardDescription>
          These recordings did not stop cleanly. Recovering keeps everything
          written up to the last complete fragment.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {bundles.map((bundle) => (
          <UnfinishedRow
            key={bundle.path}
            bundle={bundle}
            busy={busy}
            onRecover={() =>
              run(() => commands.recoverBundle(bundle.path), onRecovered)
            }
            onDiscard={() =>
              run(() => commands.discardUnfinished(bundle.path), onDiscarded)
            }
          />
        ))}
        {error && <p className="text-destructive text-sm">{error}</p>}
      </CardContent>
    </Card>
  );
}

function SavedCard({ result }: { result: FinishedRecording }) {
  const { recording } = result.project;
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          {recording.recovered ? "Recovered" : "Saved"}{" "}
          {formatElapsed(recording.durationMs)}
        </CardTitle>
        <CardDescription className="break-all">
          {result.bundlePath}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex items-center justify-between gap-2 text-sm">
        <span className="text-muted-foreground">
          {recording.width}×{recording.height} ·{" "}
          {recording.systemAudio ? "system audio" : "no system audio"} ·{" "}
          {recording.mic ? "mic" : "no mic"}
        </span>
        <div className="flex shrink-0 gap-2">
          <Button
            size="sm"
            variant="outline"
            onClick={() => commands.revealInFinder(result.bundlePath)}
          >
            <FolderOpen /> Show in Finder
          </Button>
          <Button
            size="sm"
            onClick={() => commands.openEditor(result.bundlePath)}
          >
            <Pencil /> Edit
          </Button>
        </div>
      </CardContent>
      {result.warnings.length > 0 && (
        <CardContent className="space-y-1">
          {result.warnings.map((warning) => (
            <p key={warning} className="text-sm text-amber-600">
              {warning}
            </p>
          ))}
        </CardContent>
      )}
    </Card>
  );
}

function RecordingsCard({ projects }: { projects: ProjectSummary[] }) {
  const [error, setError] = useState<string | null>(null);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Recordings</CardTitle>
        <CardDescription>Newest first</CardDescription>
      </CardHeader>
      <CardContent className="space-y-1">
        {projects.length === 0 && (
          <p className="text-muted-foreground text-sm">No recordings yet.</p>
        )}
        {projects.map((project) => (
          <div
            key={project.path}
            className="flex items-center justify-between gap-2"
          >
            <div className="min-w-0">
              <p className="truncate text-sm" title={project.path}>
                {project.name}
              </p>
              <p className="text-muted-foreground text-xs">
                {formatElapsed(project.durationMs ?? 0)} · {project.width}×
                {project.height}
              </p>
            </div>
            <Button
              size="sm"
              variant="outline"
              onClick={async () => {
                const result = await commands.openEditor(project.path);
                setError(result.status === "error" ? result.error : null);
              }}
            >
              <Pencil /> Edit
            </Button>
          </div>
        ))}
        {error && <p className="text-destructive text-sm">{error}</p>}
      </CardContent>
    </Card>
  );
}

function ScreenshotsCard({ shots }: { shots: ScreenshotSummary[] }) {
  const [error, setError] = useState<string | null>(null);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Screenshots</CardTitle>
        <CardDescription>Newest first</CardDescription>
      </CardHeader>
      <CardContent className="space-y-1">
        {shots.length === 0 && (
          <p className="text-muted-foreground text-sm">No screenshots yet.</p>
        )}
        {shots.map((shot) => (
          <div
            key={shot.path}
            className="flex items-center justify-between gap-2"
          >
            <div className="min-w-0">
              <p className="truncate text-sm" title={shot.path}>
                {shot.name}
              </p>
              <p className="text-muted-foreground text-xs">
                {shot.width}×{shot.height}
              </p>
            </div>
            <div className="flex shrink-0 gap-2">
              <Button
                size="icon"
                variant="ghost"
                aria-label={`Show ${shot.name} in Finder`}
                onClick={() => void commands.revealInFinder(shot.path)}
              >
                <FolderOpen />
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={async () => {
                  const result = await commands.editScreenshot(shot.path);
                  setError(result.status === "error" ? result.error : null);
                }}
              >
                <Pencil /> Edit
              </Button>
            </div>
          </div>
        ))}
        {error && <p className="text-destructive text-sm">{error}</p>}
      </CardContent>
    </Card>
  );
}

type Outcome = { status: "ok" } | { status: "error"; error: string };

const MODES: { mode: PickMode; label: string; Icon: typeof Crop }[] = [
  { mode: "area", label: "Area", Icon: Crop },
  { mode: "window", label: "Window", Icon: AppWindow },
  { mode: "display", label: "Display", Icon: Monitor },
];

function Actions({ phase }: { phase: Phase }) {
  const [error, setError] = useState<string | null>(null);
  const run = async (action: () => Promise<Outcome>) => {
    setError(null);
    const result = await action();
    if (result.status === "error") setError(result.error);
  };
  const active =
    phase.kind === "recording" ||
    phase.kind === "countdown" ||
    phase.kind === "starting";
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-muted-foreground w-16 text-sm">Record</span>
        {active ? (
          <Button
            variant="destructive"
            onClick={() => void commands.stopRecording()}
          >
            <Square />
            {phase.kind === "recording"
              ? `Stop ${formatElapsed(phase.elapsedMs)}`
              : phase.kind === "countdown"
                ? "Cancel Countdown"
                : "Cancel"}
          </Button>
        ) : (
          MODES.map(({ mode, label, Icon }) => (
            <Button
              key={mode}
              variant="outline"
              size="sm"
              aria-label={`Record ${label}`}
              disabled={phase.kind !== "idle"}
              onClick={() => void run(() => commands.startRecording(mode))}
            >
              <Icon /> {label}
            </Button>
          ))
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-muted-foreground w-16 text-sm">Capture</span>
        {MODES.map(({ mode, label, Icon }) => (
          <Button
            key={mode}
            variant="outline"
            size="sm"
            aria-label={`Capture ${label}`}
            onClick={() => void run(() => commands.takeScreenshot(mode))}
          >
            <Icon /> {label}
          </Button>
        ))}
      </div>
      {error && <p className="text-destructive text-sm">{error}</p>}
    </div>
  );
}

export function Library() {
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [saved, setSaved] = useState<FinishedRecording | null>(null);
  const [unfinished, setUnfinished] = useState<UnfinishedBundle[]>([]);
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [shots, setShots] = useState<ScreenshotSummary[]>([]);

  const refresh = useCallback(async () => {
    const result = await commands.listUnfinished();
    if (result.status === "ok") setUnfinished(result.data);
    const listed = await commands.listProjects();
    if (listed.status === "ok") setProjects(listed.data);
    const screenshots = await commands.listScreenshots();
    if (screenshots.status === "ok") setShots(screenshots.data);
    const current = await commands.recordingPhase();
    if (current.status === "ok") setPhase(current.data);
  }, []);

  useEffect(() => {
    void refresh();
    const changed = events.recordingChanged.listen((event) => {
      setPhase(event.payload.phase);
      if (event.payload.phase.kind === "idle") void refresh();
    });
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      void changed.then((unlisten) => unlisten());
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh]);

  const recording = phase.kind === "recording";
  useEffect(() => {
    if (!recording) return;
    const timer = setInterval(async () => {
      const current = await commands.recordingPhase();
      if (current.status === "ok") setPhase(current.data);
    }, 500);
    return () => clearInterval(timer);
  }, [recording]);

  return (
    <main className="mx-auto max-w-xl space-y-4 p-4">
      <div className="flex items-start justify-between gap-2">
        <h1 className="text-xl font-semibold">Library</h1>
        <div className="flex items-start gap-2">
          <Button
            variant="outline"
            size="icon"
            aria-label="Open the recordings folder"
            onClick={() => void commands.openRecordingsFolder()}
          >
            <FolderOpen />
          </Button>
          <Button
            variant="outline"
            size="icon"
            aria-label="Settings"
            onClick={() => void commands.openWindow("settings")}
          >
            <Settings />
          </Button>
        </div>
      </div>

      <Actions phase={phase} />

      {saved && <SavedCard result={saved} />}

      <UnfinishedCard
        bundles={unfinished}
        onRecovered={(result) => {
          setSaved(result);
          void refresh();
        }}
        onDiscarded={() => void refresh()}
      />

      <RecordingsCard projects={projects} />

      <ScreenshotsCard shots={shots} />
    </main>
  );
}
