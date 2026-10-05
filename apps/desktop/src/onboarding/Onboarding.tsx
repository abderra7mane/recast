import { useCallback, useEffect, useState } from "react";
import { CheckCircle2, Circle, RotateCw, TriangleAlert } from "lucide-react";

import { commands, type Permission, type Permissions } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  PERMISSIONS,
  STATE_LABEL,
  actionFor,
  canFinish,
  type PermissionInfo,
} from "@/onboarding/permissions";

const RECHECK_MS = 1500;

function PermissionRow({
  info,
  permissions,
  onAct,
}: {
  info: PermissionInfo;
  permissions: Permissions | null;
  onAct: (id: Permission) => void;
}) {
  const state = permissions?.[info.id];
  const granted = state === "granted";
  const action = actionFor(info.id, state);
  return (
    <div className="space-y-2 rounded-lg border p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-3">
          {granted ? (
            <CheckCircle2 className="mt-0.5 size-5 shrink-0 text-green-600" />
          ) : (
            <Circle className="text-muted-foreground mt-0.5 size-5 shrink-0" />
          )}
          <div className="space-y-0.5">
            <p className="font-medium">
              {info.title}{" "}
              <span className="text-muted-foreground text-xs font-normal">
                {info.need}
              </span>
            </p>
            <p className="text-muted-foreground text-sm">{info.why}</p>
          </div>
        </div>
        <div className="flex shrink-0 flex-col items-end gap-2">
          {state && (
            <Badge variant={granted ? "default" : "outline"}>
              {STATE_LABEL[state]}
            </Badge>
          )}
          {action !== "none" && (
            <Button size="sm" variant="outline" onClick={() => onAct(info.id)}>
              {action === "request" ? "Request" : "Open System Settings"}
            </Button>
          )}
        </div>
      </div>
      {state && !granted && info.need !== "optional" && (
        <p className="flex items-start gap-1.5 pl-8 text-xs text-amber-600">
          <TriangleAlert className="mt-px size-3.5 shrink-0" />
          {info.without}
        </p>
      )}
    </div>
  );
}

export function Onboarding() {
  const [permissions, setPermissions] = useState<Permissions | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setPermissions(await commands.checkPermissions());
  }, []);

  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), RECHECK_MS);
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh]);

  const act = async (id: Permission) => {
    const action = actionFor(id, permissions?.[id]);
    // Asking first also adds Recast to the list in System Settings.
    await commands.requestPermission(id);
    if (action === "openSettings") {
      const result = await commands.openPrivacySettings(id);
      if (result.status === "error") setError(result.error);
    }
    await refresh();
  };

  const finish = async () => {
    const result = await commands.completeOnboarding();
    if (result.status === "error") setError(result.error);
  };

  const relaunch = async () => {
    const result = await commands.relaunch();
    if (result.status === "error") setError(result.error);
  };

  return (
    <main className="flex min-h-screen flex-col gap-4 p-6">
      <div className="space-y-1">
        <h1 className="text-xl font-semibold">Welcome to Recast</h1>
        <p className="text-muted-foreground text-sm">
          Recast lives in the menu bar. Give it these permissions to record your
          screen and take screenshots.
        </p>
      </div>

      <div className="space-y-3">
        {PERMISSIONS.map((info) => (
          <PermissionRow
            key={info.id}
            info={info}
            permissions={permissions}
            onAct={(id) => void act(id)}
          />
        ))}
      </div>

      <div className="bg-muted flex items-center justify-between gap-3 rounded-lg p-3 text-sm">
        <p className="text-muted-foreground">
          macOS applies Screen Recording only after Recast restarts. Granted it
          already? Relaunch to finish.
        </p>
        <Button size="sm" variant="outline" onClick={() => void relaunch()}>
          <RotateCw /> Relaunch Recast
        </Button>
      </div>

      {error && <p className="text-destructive text-sm">{error}</p>}

      <div className="mt-auto flex justify-end">
        <Button
          disabled={!canFinish(permissions)}
          onClick={() => void finish()}
        >
          Done
        </Button>
      </div>
    </main>
  );
}
