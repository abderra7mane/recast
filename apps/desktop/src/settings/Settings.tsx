import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Check, Copy, FolderOpen, RefreshCw } from "lucide-react";

import {
  commands,
  type AppInfo,
  type AppSettings,
  type LoginItem,
  type ShortcutAction,
  type ShortcutStatus,
} from "@/bindings";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  DEFAULT_RECORDINGS_FOLDER,
  DEFAULT_SCREENSHOTS_FOLDER,
  prefs,
  type Prefs,
} from "@/settings/prefs";
import { ShortcutRecorder } from "@/settings/ShortcutRecorder";

const SHORTCUTS: { action: ShortcutAction; label: string }[] = [
  { action: "recordArea", label: "Record area" },
  { action: "recordWindow", label: "Record window" },
  { action: "recordDisplay", label: "Record display" },
  { action: "captureArea", label: "Capture area" },
  { action: "captureWindow", label: "Capture window" },
  { action: "captureDisplay", label: "Capture display" },
];

function Toggle({
  id,
  label,
  description,
  checked,
  onChange,
}: {
  id: string;
  label: string;
  description?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div className="space-y-0.5">
        <Label htmlFor={id}>{label}</Label>
        {description && (
          <p className="text-muted-foreground text-xs">{description}</p>
        )}
      </div>
      <Switch id={id} checked={checked} onCheckedChange={onChange} />
    </div>
  );
}

function FolderField({
  label,
  folder,
  fallback,
  onChange,
}: {
  label: string;
  folder: string | null;
  fallback: string;
  onChange: (folder: string | null) => void;
}) {
  const choose = async () => {
    const picked = await open({
      directory: true,
      defaultPath: folder ?? undefined,
      title: label,
    });
    if (typeof picked === "string") onChange(picked);
  };
  return (
    <div className="space-y-1.5">
      <Label>{label}</Label>
      <div className="flex items-center gap-2">
        <span
          className="bg-muted min-w-0 flex-1 truncate rounded-md px-2 py-1.5 font-mono text-xs"
          title={folder ?? fallback}
        >
          {folder ?? fallback}
        </span>
        <Button size="sm" variant="outline" onClick={() => void choose()}>
          Change…
        </Button>
        {folder && (
          <Button size="sm" variant="ghost" onClick={() => onChange(null)}>
            Use Default
          </Button>
        )}
      </div>
    </div>
  );
}

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: string };

export function Settings() {
  const [settings, setSettings] = useState<Prefs | null>(null);
  const [statuses, setStatuses] = useState<ShortcutStatus[]>([]);
  const [loginItem, setLoginItem] = useState<LoginItem | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    void commands.getSettings().then((result) => {
      if (result.status === "ok") setSettings(prefs(result.data));
      else setError(result.error);
    });
    void commands.shortcutStatuses().then(setStatuses);
    void commands.getLaunchAtLogin().then(setLoginItem);
    void commands.appInfo().then(setInfo);
  }, []);

  const apply = useCallback(
    async (save: () => Promise<Result<AppSettings>>, optimistic: Prefs) => {
      setSettings(optimistic);
      setError(null);
      const result = await save();
      if (result.status === "ok") {
        setSettings(prefs(result.data));
      } else {
        setError(result.error);
      }
    },
    [],
  );

  if (!settings) {
    return (
      <main className="text-muted-foreground p-6 text-sm">
        {error ?? "Loading…"}
      </main>
    );
  }

  const recording = (patch: Partial<Prefs["recording"]>) => {
    const next = { ...settings.recording, ...patch };
    void apply(() => commands.setRecordingSettings(next), {
      ...settings,
      recording: next,
    });
  };
  const screenshots = (patch: Partial<Prefs["screenshots"]>) => {
    const next = { ...settings.screenshots, ...patch };
    void apply(() => commands.setScreenshotSettings(next), {
      ...settings,
      screenshots: next,
    });
  };
  const updates = (patch: Partial<Prefs["updates"]>) => {
    const next = { ...settings.updates, ...patch };
    void apply(() => commands.setUpdateSettings(next), {
      ...settings,
      updates: next,
    });
  };

  const changeShortcut = async (
    action: ShortcutAction,
    shortcut: string | null,
  ) => {
    const result = await commands.setShortcut(action, shortcut);
    if (result.status === "error") return result.error;
    setStatuses(result.data);
    setSettings({
      ...settings,
      shortcuts: {
        ...settings.shortcuts,
        [action]:
          result.data.find((s) => s.action === action)?.shortcut ?? null,
      },
    });
    return null;
  };

  const changeLoginItem = async (enabled: boolean) => {
    const result = await commands.setLaunchAtLogin(enabled);
    if (result.status === "ok") {
      setLoginItem(result.data);
      setError(null);
    } else {
      setError(`Launch at login: ${result.error}`);
    }
  };

  const copyDiagnostics = async () => {
    const result = await commands.copyDiagnostics();
    if (result.status === "ok") {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } else {
      setError(result.error);
    }
  };

  return (
    <main className="space-y-4 p-6">
      <Tabs defaultValue="general">
        <TabsList className="w-full">
          <TabsTrigger value="general">General</TabsTrigger>
          <TabsTrigger value="shortcuts">Shortcuts</TabsTrigger>
          <TabsTrigger value="recording">Recording</TabsTrigger>
          <TabsTrigger value="screenshots">Screenshots</TabsTrigger>
          <TabsTrigger value="updates">Updates</TabsTrigger>
          <TabsTrigger value="about">About</TabsTrigger>
        </TabsList>

        <TabsContent value="general" className="space-y-4 pt-2">
          <Toggle
            id="launch-at-login"
            label="Launch at login"
            description="Keep Recast in the menu bar from the moment you log in."
            checked={loginItem === "enabled" || loginItem === "needsApproval"}
            onChange={(enabled) => void changeLoginItem(enabled)}
          />
          {loginItem === "needsApproval" && (
            <div className="flex items-center justify-between gap-2 text-xs text-amber-600">
              <span>Allow Recast in System Settings → Login Items.</span>
              <Button
                size="xs"
                variant="outline"
                onClick={() => void commands.openLoginItemsSettings()}
              >
                Open Login Items
              </Button>
            </div>
          )}
          <Separator />
          <div className="flex items-center justify-between gap-4">
            <div className="space-y-0.5">
              <Label>Permissions</Label>
              <p className="text-muted-foreground text-xs">
                Check Screen Recording, Input Monitoring and Microphone access.
              </p>
            </div>
            <Button
              size="sm"
              variant="outline"
              onClick={() => void commands.openWindow("onboarding")}
            >
              Show Onboarding
            </Button>
          </div>
        </TabsContent>

        <TabsContent value="shortcuts" className="space-y-4 pt-2">
          <p className="text-muted-foreground text-xs">
            These work in any app. Click a shortcut to record a new one.
          </p>
          {SHORTCUTS.map(({ action, label }) => {
            const status = statuses.find((s) => s.action === action);
            return (
              <ShortcutRecorder
                key={action}
                label={label}
                shortcut={settings.shortcuts[action]}
                error={status?.error}
                onChange={(shortcut) => changeShortcut(action, shortcut)}
                onRecording={(recordingShortcut) =>
                  void commands.suspendShortcuts(recordingShortcut)
                }
              />
            );
          })}
        </TabsContent>

        <TabsContent value="recording" className="space-y-4 pt-2">
          <Toggle
            id="countdown"
            label="Countdown"
            description="Count down from 3 before recording. Click or press Esc to skip it."
            checked={settings.recording.countdown}
            onChange={(countdown) => recording({ countdown })}
          />
          <Toggle
            id="system-audio"
            label="Record system audio"
            checked={settings.recording.systemAudio}
            onChange={(systemAudio) => recording({ systemAudio })}
          />
          <Toggle
            id="mic"
            label="Record microphone"
            checked={settings.recording.mic}
            onChange={(mic) => recording({ mic })}
          />
          <Separator />
          <FolderField
            label="Save recordings to"
            folder={settings.recording.folder}
            fallback={DEFAULT_RECORDINGS_FOLDER}
            onChange={(folder) => recording({ folder })}
          />
        </TabsContent>

        <TabsContent value="screenshots" className="space-y-4 pt-2">
          <Toggle
            id="copy-to-clipboard"
            label="Copy to the clipboard"
            checked={settings.screenshots.copyToClipboard}
            onChange={(copyToClipboard) => screenshots({ copyToClipboard })}
          />
          <Toggle
            id="save-to-disk"
            label="Save every screenshot"
            checked={settings.screenshots.saveToDisk}
            onChange={(saveToDisk) => screenshots({ saveToDisk })}
          />
          <FolderField
            label="Save screenshots to"
            folder={settings.screenshots.folder}
            fallback={DEFAULT_SCREENSHOTS_FOLDER}
            onChange={(folder) => screenshots({ folder })}
          />
          <Toggle
            id="play-shutter-sound"
            label="Play shutter sound"
            checked={settings.screenshots.playShutterSound}
            onChange={(playShutterSound) => screenshots({ playShutterSound })}
          />
        </TabsContent>

        <TabsContent value="updates" className="space-y-4 pt-2">
          <Toggle
            id="check-automatically"
            label="Check for updates automatically"
            description="Recast checks when it starts."
            checked={settings.updates.checkAutomatically}
            onChange={(checkAutomatically) => updates({ checkAutomatically })}
          />
          <Button
            variant="outline"
            onClick={() => void commands.checkForUpdates()}
          >
            <RefreshCw /> Check Now
          </Button>
        </TabsContent>

        <TabsContent value="about" className="space-y-4 pt-2">
          <div>
            <p className="text-lg font-semibold">Recast</p>
            <p className="text-muted-foreground text-sm">
              Version {info?.version ?? "…"}
            </p>
          </div>
          <div className="space-y-1.5">
            <Label>Logs</Label>
            <Button
              variant="link"
              className="h-auto p-0 font-mono text-xs"
              onClick={() => void commands.openLogsFolder()}
            >
              <FolderOpen /> {info?.logsDir ?? "~/Library/Logs/Recast"}
            </Button>
          </div>
          <div className="space-y-1.5">
            <Button variant="outline" onClick={() => void copyDiagnostics()}>
              {copied ? <Check /> : <Copy />}
              {copied ? "Copied" : "Copy Diagnostics"}
            </Button>
            <p className="text-muted-foreground text-xs">
              Copies the version, macOS version, Mac model, permissions and the
              last 200 log lines, with your home folder hidden.
            </p>
          </div>
        </TabsContent>
      </Tabs>
      {error && (
        <p role="alert" className="text-destructive text-sm">
          {error}
        </p>
      )}
    </main>
  );
}
