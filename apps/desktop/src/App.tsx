import { useCallback, useEffect, useState } from "react";
import { Circle, FolderOpen, RefreshCw, Square } from "lucide-react";

import {
  commands,
  type DisplayInfo,
  type FinishedRecording,
  type Permission,
  type Permissions,
  type PermissionState,
  type RecordingStatus,
  type UnfinishedBundle,
  type WindowInfo,
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
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  buildTarget,
  formatElapsed,
  windowLabel,
  type RegionInput,
  type SourceForm,
  type SourceKind,
} from "@/recording";

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

type SourcesState = {
  displays: DisplayInfo[];
  windows: WindowInfo[];
  error: string | null;
};

function useSources() {
  const [sources, setSources] = useState<SourcesState>({
    displays: [],
    windows: [],
    error: null,
  });

  const refresh = useCallback(async () => {
    const [displays, windows] = await Promise.all([
      commands.listDisplays(),
      commands.listWindows(),
    ]);
    setSources({
      displays: displays.status === "ok" ? displays.data : [],
      windows: windows.status === "ok" ? windows.data : [],
      error:
        displays.status === "error"
          ? displays.error
          : windows.status === "error"
            ? windows.error
            : null,
    });
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { ...sources, refresh };
}

function RegionFields({
  region,
  onChange,
  disabled,
}: {
  region: RegionInput;
  onChange: (region: RegionInput) => void;
  disabled: boolean;
}) {
  const fields: (keyof RegionInput)[] = ["x", "y", "width", "height"];
  return (
    <div className="grid grid-cols-4 gap-2">
      {fields.map((field) => (
        <div key={field} className="space-y-1">
          <Label htmlFor={`region-${field}`}>{field}</Label>
          <Input
            id={`region-${field}`}
            inputMode="decimal"
            value={region[field]}
            disabled={disabled}
            onChange={(e) => onChange({ ...region, [field]: e.target.value })}
          />
        </div>
      ))}
    </div>
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
        <Button
          size="sm"
          variant="outline"
          onClick={() => commands.revealInFinder(result.bundlePath)}
        >
          <FolderOpen /> Show in Finder
        </Button>
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

export default function App() {
  const sources = useSources();
  const [form, setForm] = useState<SourceForm>({
    kind: "display",
    displayId: null,
    windowId: null,
    region: { x: "0", y: "0", width: "1280", height: "720" },
  });
  const [systemAudio, setSystemAudio] = useState(true);
  const [mic, setMic] = useState(false);
  const [status, setStatus] = useState<RecordingStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<FinishedRecording | null>(null);
  const [unfinished, setUnfinished] = useState<UnfinishedBundle[]>([]);

  const refreshUnfinished = useCallback(async () => {
    const result = await commands.listUnfinished();
    if (result.status === "ok") setUnfinished(result.data);
  }, []);

  useEffect(() => {
    void refreshUnfinished();
    void commands.recordingStatus().then((r) => {
      if (r.status === "ok") setStatus(r.data);
    });
  }, [refreshUnfinished]);

  const displayId = form.displayId ?? sources.displays[0]?.id ?? null;
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
    const target = buildTarget({ ...form, displayId }, sources.displays);
    if (!target.ok) {
      setError(target.error);
      return;
    }
    setBusy(true);
    const result = await commands.startRecording({
      target: target.target,
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

  const setKind = (kind: SourceKind) => setForm({ ...form, kind });

  return (
    <main className="mx-auto max-w-xl space-y-4 p-4">
      <h1 className="text-xl font-semibold">Recast</h1>

      <PermissionsCard />

      <Card>
        <CardHeader>
          <CardTitle>Source</CardTitle>
          {sources.error && (
            <CardDescription className="text-destructive">
              {sources.error}
            </CardDescription>
          )}
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex gap-2">
            {(["display", "window", "region"] as const).map((kind) => (
              <Button
                key={kind}
                size="sm"
                variant={form.kind === kind ? "default" : "outline"}
                disabled={recording}
                onClick={() => setKind(kind)}
              >
                {kind[0].toUpperCase() + kind.slice(1)}
              </Button>
            ))}
            <Button
              size="sm"
              variant="ghost"
              className="ml-auto"
              disabled={recording}
              onClick={() => sources.refresh()}
              aria-label="Refresh sources"
            >
              <RefreshCw />
            </Button>
          </div>

          {form.kind !== "window" && (
            <div className="space-y-1">
              <Label>Display</Label>
              <Select
                value={displayId === null ? undefined : String(displayId)}
                disabled={recording}
                onValueChange={(v) =>
                  setForm({ ...form, displayId: Number(v) })
                }
              >
                <SelectTrigger className="w-full">
                  <SelectValue placeholder="Pick a display" />
                </SelectTrigger>
                <SelectContent>
                  {sources.displays.map((d) => (
                    <SelectItem key={d.id} value={String(d.id)}>
                      {d.name} — {d.bounds.width}×{d.bounds.height} pt @
                      {d.scaleFactor}x
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}

          {form.kind === "window" && (
            <div className="space-y-1">
              <Label>Window</Label>
              <Select
                value={
                  form.windowId === null ? undefined : String(form.windowId)
                }
                disabled={recording}
                onValueChange={(v) => setForm({ ...form, windowId: Number(v) })}
              >
                <SelectTrigger className="w-full">
                  <SelectValue placeholder="Pick a window" />
                </SelectTrigger>
                <SelectContent>
                  {sources.windows.map((w) => (
                    <SelectItem key={w.id} value={String(w.id)}>
                      {windowLabel(w)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}

          {form.kind === "region" && (
            <RegionFields
              region={form.region}
              disabled={recording}
              onChange={(region) => setForm({ ...form, region })}
            />
          )}

          <div className="flex items-center gap-6">
            <div className="flex items-center gap-2">
              <Switch
                id="system-audio"
                checked={systemAudio}
                disabled={recording}
                onCheckedChange={setSystemAudio}
              />
              <Label htmlFor="system-audio">System audio</Label>
            </div>
            <div className="flex items-center gap-2">
              <Switch
                id="mic"
                checked={mic}
                disabled={recording}
                onCheckedChange={setMic}
              />
              <Label htmlFor="mic">Microphone</Label>
            </div>
          </div>
        </CardContent>
      </Card>

      <div className="flex items-center gap-4">
        {recording ? (
          <Button variant="destructive" disabled={busy} onClick={stop}>
            <Square /> Stop
          </Button>
        ) : (
          <Button disabled={busy} onClick={start}>
            <Circle /> Start recording
          </Button>
        )}
        {status && (
          <span className="font-mono text-sm">
            {formatElapsed(status.elapsedMs)} · {status.width}×{status.height}
          </span>
        )}
      </div>

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
    </main>
  );
}
