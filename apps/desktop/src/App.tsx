import { useCallback, useEffect, useState } from "react";
import {
  AppWindow,
  Circle,
  Crop,
  FolderOpen,
  Monitor,
  Pencil,
  Square,
} from "lucide-react";

import {
  commands,
  type FinishedRecording,
  type Permission,
  type Permissions,
  type PermissionState,
  type PickMode,
  type ProjectSummary,
  type RecordingStatus,
  type UnfinishedBundle,
} from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { complete, type Complete } from "@/editor/settings";
import type { ScreenshotSettings } from "@/bindings";
import { formatElapsed, pickedLabel, screenshotMessage } from "@/recording";

const PERMISSIONS: { key: keyof Permissions; id: Permission; label: string }[] =
  [
    {
      key: "screenRecording",
      id: "screenRecording",
      label: "Screen Recording",
    },
    {
      key: "inputMonitoring",
      id: "inputMonitoring",
      label: "Input Monitoring",
    },
    { key: "microphone", id: "microphone", label: "Microphone" },
  ];

const STATE_LABEL: Record<PermissionState, string> = {
  granted: "Granted",
  denied: "Not granted",
  notDetermined: "Not asked",
};

function PermissionsCard() {
  const [permissions, setPermissions] = useState<Permissions | null>(null);

  const refresh = useCallback(async () => {
    setPermissions(await commands.checkPermissions());
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const request = async (permission: Permission) => {
    await commands.requestPermission(permission);
    await refresh();
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Permissions</CardTitle>
        <CardDescription>
          Changes in System Settings may need a restart of Recast.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-2">
        {PERMISSIONS.map(({ key, id, label }) => {
          const state = permissions?.[key];
          return (
            <div key={id} className="flex items-center justify-between gap-2">
              <span className="text-sm">{label}</span>
              <div className="flex items-center gap-2">
                {state && (
                  <Badge variant={state === "granted" ? "default" : "outline"}>
                    {STATE_LABEL[state]}
                  </Badge>
                )}
                {state && state !== "granted" && (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => request(id)}
                  >
                    Request
                  </Button>
                )}
              </div>
            </div>
          );
        })}
      </CardContent>
    </Card>
  );
}

function ToggleField({
  id,
  label,
  checked,
  disabled,
  onChange,
}: {
  id: string;
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <Switch
        id={id}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
      />
      <Label htmlFor={id}>{label}</Label>
    </div>
  );
}

const CAPTURES: { mode: PickMode; label: string; icon: typeof Crop }[] = [
  { mode: "area", label: "Capture area", icon: Crop },
  { mode: "window", label: "Capture window", icon: AppWindow },
  { mode: "display", label: "Capture display", icon: Monitor },
];

function ScreenshotsCard() {
  const [options, setOptions] = useState<Complete<ScreenshotSettings> | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{
    text: string;
    error?: boolean;
  } | null>(null);

  useEffect(() => {
    void commands.getSettings().then((result) => {
      if (result.status === "ok") setOptions(complete(result.data).screenshots);
    });
  }, []);

  const change = async (patch: Partial<ScreenshotSettings>) => {
    if (!options) return;
    const next = { ...options, ...patch };
    setOptions(next);
    const result = await commands.setScreenshotSettings(next);
    if (result.status === "ok") setOptions(complete(result.data).screenshots);
    else setMessage({ text: result.error, error: true });
  };

  const capture = async (mode: PickMode) => {
    setBusy(true);
    setMessage(null);
    const result = await commands.takeScreenshot(mode);
    setBusy(false);
    if (result.status === "error") {
      setMessage({ text: result.error, error: true });
    } else if (result.data) {
      const warnings = result.data.warnings.join(" ");
      setMessage({
        text: [screenshotMessage(result.data), warnings].join(" ").trim(),
        error: warnings.length > 0,
      });
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Screenshots</CardTitle>
        <CardDescription>
          Drag to select an area or click a window. Space switches to the whole
          display, Esc cancels.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap gap-2">
          {CAPTURES.map(({ mode, label, icon: Icon }) => (
            <Button
              key={mode}
              variant="outline"
              disabled={busy}
              onClick={() => capture(mode)}
            >
              <Icon /> {label}
            </Button>
          ))}
        </div>
        {options && (
          <div className="flex flex-wrap items-center gap-6">
            <ToggleField
              id="copy-to-clipboard"
              label="Copy to clipboard"
              checked={options.copyToClipboard}
              onChange={(copyToClipboard) => change({ copyToClipboard })}
            />
            <ToggleField
              id="save-to-disk"
              label="Save to ~/Pictures/Recast"
              checked={options.saveToDisk}
              onChange={(saveToDisk) => change({ saveToDisk })}
            />
          </div>
        )}
        {message && (
          <p
            className={
              message.error
                ? "text-destructive text-sm break-all"
                : "text-muted-foreground text-sm break-all"
            }
          >
            {message.text}
          </p>
        )}
      </CardContent>
    </Card>
  );
}

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
        <CardDescription>In ~/Movies/Recast</CardDescription>
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

export default function App() {
  const [systemAudio, setSystemAudio] = useState(true);
  const [mic, setMic] = useState(false);
  const [target, setTarget] = useState<string | null>(null);
  const [status, setStatus] = useState<RecordingStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<FinishedRecording | null>(null);
  const [unfinished, setUnfinished] = useState<UnfinishedBundle[]>([]);
  const [projects, setProjects] = useState<ProjectSummary[]>([]);

  const refreshUnfinished = useCallback(async () => {
    const result = await commands.listUnfinished();
    if (result.status === "ok") setUnfinished(result.data);
    setProjects(await commands.listProjects());
  }, []);

  useEffect(() => {
    void refreshUnfinished();
    void commands.recordingStatus().then((r) => {
      if (r.status === "ok") setStatus(r.data);
    });
  }, [refreshUnfinished]);

  const recording = status !== null;

  useEffect(() => {
    if (!recording) return;
    const timer = setInterval(async () => {
      const r = await commands.recordingStatus();
      if (r.status === "ok") setStatus(r.data);
    }, 500);
    return () => clearInterval(timer);
  }, [recording]);

  const start = async () => {
    setError(null);
    setBusy(true);
    const picked = await commands.pickTarget("window");
    if (picked.status === "error" || !picked.data) {
      setBusy(false);
      if (picked.status === "error") setError(picked.error);
      return;
    }
    setTarget(pickedLabel(picked.data));
    const result = await commands.startRecording({
      target: picked.data.target,
      systemAudio,
      mic,
    });
    setBusy(false);
    if (result.status === "ok") {
      setStatus(result.data);
      setSaved(null);
    } else {
      setError(result.error);
    }
  };

  const stop = async () => {
    setBusy(true);
    const result = await commands.stopRecording();
    setBusy(false);
    setStatus(null);
    if (result.status === "ok") setSaved(result.data);
    else setError(result.error);
    void refreshUnfinished();
  };

  return (
    <main className="mx-auto max-w-xl space-y-4 p-4">
      <h1 className="text-xl font-semibold">Recast</h1>

      <PermissionsCard />

      <ScreenshotsCard />

      <Card>
        <CardHeader>
          <CardTitle>Recording</CardTitle>
          <CardDescription>
            Record… lets you pick a window, drag an area or press Space for the
            whole display.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center gap-6">
            <ToggleField
              id="system-audio"
              label="System audio"
              checked={systemAudio}
              disabled={recording}
              onChange={setSystemAudio}
            />
            <ToggleField
              id="mic"
              label="Microphone"
              checked={mic}
              disabled={recording}
              onChange={setMic}
            />
          </div>
          <div className="flex items-center gap-4">
            {recording ? (
              <Button variant="destructive" disabled={busy} onClick={stop}>
                <Square /> Stop
              </Button>
            ) : (
              <Button disabled={busy} onClick={start}>
                <Circle /> Record…
              </Button>
            )}
            {status && (
              <span className="font-mono text-sm">
                {formatElapsed(status.elapsedMs)} · {status.width}×
                {status.height}
              </span>
            )}
          </div>
          {recording && target && (
            <p className="text-muted-foreground truncate text-sm">{target}</p>
          )}
        </CardContent>
      </Card>

      {status && !status.inputEvents && (
        <p className="text-sm text-amber-600">
          Input Monitoring is not granted: clicks and cursor moves are not
          recorded.
        </p>
      )}
      {status?.problems.map((problem) => (
        <p key={problem} className="text-destructive text-sm">
          {problem}
        </p>
      ))}
      {error && <p className="text-destructive text-sm">{error}</p>}

      {saved && <SavedCard result={saved} />}

      <UnfinishedCard
        bundles={unfinished}
        onRecovered={(result) => {
          setSaved(result);
          void refreshUnfinished();
        }}
        onDiscarded={() => void refreshUnfinished()}
      />

      <RecordingsCard projects={projects} />
    </main>
  );
}
