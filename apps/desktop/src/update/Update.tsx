import { useEffect, useState, type ReactNode } from "react";
import {
  ArrowRight,
  CircleAlert,
  CircleCheck,
  Download,
  LoaderCircle,
  RotateCw,
} from "lucide-react";

import {
  commands,
  events,
  type UpdateInfo,
  type UpdateStatus,
} from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import icon from "../../src-tauri/icons/128x128@2x.png";

type DownloadState = { downloaded: number; total: number | null };

function megabytes(bytes: number) {
  return (bytes / 1_000_000).toFixed(1);
}

function DownloadProgress({ downloaded, total }: DownloadState) {
  return (
    <div className="space-y-1.5">
      <Progress value={total ? (downloaded / total) * 100 : undefined} />
      <p className="text-muted-foreground text-xs tabular-nums">
        {total
          ? `Downloading… ${megabytes(downloaded)} of ${megabytes(total)} MB`
          : `Downloading… ${megabytes(downloaded)} MB`}
      </p>
    </div>
  );
}

function close() {
  void commands.dismissUpdate();
}

/** A centered icon, title and message, for every state but "available". */
function Notice({
  badge,
  title,
  children,
  actions,
}: {
  badge?: ReactNode;
  title: string;
  children: ReactNode;
  actions: ReactNode;
}) {
  return (
    <main className="flex h-full flex-col p-6 select-none">
      <div className="flex flex-1 flex-col items-center justify-center gap-4 text-center">
        <div className="relative">
          <img src={icon} alt="" className="size-20" />
          {badge && (
            <span className="bg-background absolute -right-1 -bottom-1 rounded-full p-0.5">
              {badge}
            </span>
          )}
        </div>
        <div className="max-w-sm space-y-1.5">
          <h1 className="text-xl font-semibold">{title}</h1>
          <div className="text-muted-foreground text-sm">{children}</div>
        </div>
      </div>
      <div className="flex justify-end gap-2">{actions}</div>
    </main>
  );
}

function Available({ info }: { info: UpdateInfo }) {
  const [download, setDownload] = useState<DownloadState | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const progress = events.updateProgress.listen(({ payload }) =>
      setDownload(payload),
    );
    return () => void progress.then((off) => off());
  }, []);

  const install = async () => {
    setError(null);
    setDownload({ downloaded: 0, total: null });
    const result = await commands.installUpdate();
    if (result.status === "error") {
      setDownload(null);
      setError(result.error);
    }
  };

  const installing = download !== null;

  return (
    <main className="flex h-full flex-col gap-5 p-6 select-none">
      <div className="flex items-center gap-4">
        <img src={icon} alt="" className="size-16 shrink-0" />
        <div className="space-y-1.5">
          <h1 className="text-xl font-semibold">A new version is available</h1>
          <div className="text-muted-foreground flex items-center gap-2 text-sm">
            <Badge variant="outline">{info.currentVersion}</Badge>
            <ArrowRight className="size-3.5" />
            <Badge>{info.version}</Badge>
          </div>
        </div>
      </div>

      <div className="bg-muted/50 min-h-0 flex-1 overflow-y-auto rounded-lg border p-4 text-sm">
        {info.notes ? (
          <>
            <p className="mb-2 font-medium">What's new</p>
            <p className="text-muted-foreground whitespace-pre-wrap select-text">
              {info.notes}
            </p>
          </>
        ) : (
          <p className="text-muted-foreground">
            Recast {info.version} is ready to install. Recast relaunches to
            finish updating; your recordings and settings stay as they are.
          </p>
        )}
      </div>

      {download && <DownloadProgress {...download} />}
      {error && <p className="text-destructive text-sm">{error}</p>}

      <div className="flex justify-end gap-2">
        <Button variant="outline" disabled={installing} onClick={close}>
          Later
        </Button>
        <Button disabled={installing} onClick={() => void install()}>
          <Download /> Install and Relaunch
        </Button>
      </div>
    </main>
  );
}

function View({ status }: { status: UpdateStatus }) {
  switch (status.kind) {
    case "available":
      return <Available info={status} />;
    case "checking":
      return (
        <Notice
          badge={
            <LoaderCircle className="text-muted-foreground size-6 animate-spin" />
          }
          title="Checking for updates…"
          actions={
            <Button variant="outline" onClick={close}>
              Close
            </Button>
          }
        >
          Looking for a newer version of Recast.
        </Notice>
      );
    case "upToDate":
      return (
        <Notice
          badge={<CircleCheck className="size-6 text-success" />}
          title="You're up to date"
          actions={<Button onClick={close}>OK</Button>}
        >
          Recast {status.version} is the newest version.
        </Notice>
      );
    case "notConfigured":
      return (
        <Notice
          title="Updates aren't available"
          actions={<Button onClick={close}>OK</Button>}
        >
          Updates aren't set up for this build of Recast.
        </Notice>
      );
    case "failed":
      return (
        <Notice
          badge={<CircleAlert className="text-destructive size-6" />}
          title="Couldn't check for updates"
          actions={
            <>
              <Button variant="outline" onClick={close}>
                Close
              </Button>
              <Button onClick={() => void commands.checkForUpdates()}>
                <RotateCw /> Try Again
              </Button>
            </>
          }
        >
          <p className="select-text">{status.error}</p>
        </Notice>
      );
  }
}

export function Update() {
  const [status, setStatus] = useState<UpdateStatus | null>(null);

  useEffect(() => {
    const changed = events.updateStatusChanged.listen(({ payload }) =>
      setStatus(payload.status),
    );
    void commands.updateStatus().then((result) => {
      if (result.status === "ok") setStatus(result.data);
    });
    return () => void changed.then((off) => off());
  }, []);

  return status && <View status={status} />;
}
