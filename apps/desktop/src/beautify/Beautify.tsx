import { useEffect, useMemo, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Copy, FolderOpen, Save } from "lucide-react";

import { commands, type BeautifyInit } from "@/bindings";
import { createLatestRunner, previewSize } from "@/beautify/latest";
import { Button } from "@/components/ui/button";
import {
  BackgroundControls,
  type Background,
} from "@/editor/inspector/BackgroundControls";
import { GestureContext } from "@/editor/inspector/gesture";
import { complete } from "@/editor/settings";

const NO_GESTURE = { begin: () => {}, end: () => {} };

type Status = { text: string; path?: string; error?: boolean };
type Size = { width: number; height: number };

type CommandResult<T> =
  { status: "ok"; data: T } | { status: "error"; error: string };

export function Beautify() {
  const [init, setInit] = useState<BeautifyInit | null>(null);
  const [background, setBackground] = useState<Background | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [size, setSize] = useState<Size | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState(false);
  const area = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void commands.beautifyOpen().then((result) => {
      if (result.status === "ok") {
        setInit(result.data);
        setBackground(complete(result.data.background));
      } else {
        setStatus({ text: result.error, error: true });
      }
    });
  }, []);

  useEffect(() => {
    const element = area.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setSize(previewSize(width, height, window.devicePixelRatio));
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const runner = useMemo(
    () =>
      createLatestRunner(
        async (request: { background: Background; size: Size }) => {
          const result = await commands.beautifyPreview(
            request.background,
            request.size.width,
            request.size.height,
          );
          if (result.status === "ok") setPreview(result.data);
          else setStatus({ text: result.error, error: true });
        },
      ),
    [],
  );

  useEffect(() => {
    if (background && size) runner.push({ background, size });
  }, [background, size, runner]);

  const run = async <T,>(
    action: () => Promise<CommandResult<T>>,
    done: (data: T) => Status,
  ) => {
    setBusy(true);
    setStatus(null);
    const result = await action();
    setBusy(false);
    setStatus(
      result.status === "ok"
        ? done(result.data)
        : { text: result.error, error: true },
    );
  };

  const copy = () => {
    if (!background) return;
    void run(
      () => commands.beautifyCopy(background),
      () => ({ text: "Copied to the clipboard." }),
    );
  };

  const saveTo = async (choose: boolean) => {
    if (!background || !init) return;
    let path: string | null = null;
    if (choose) {
      path = await save({
        defaultPath: `${init.name} beautified.png`,
        filters: [{ name: "PNG image", extensions: ["png"] }],
      });
      if (!path) return;
    }
    await run(
      () => commands.beautifySave(background, path),
      (saved) => ({ text: `Saved to ${saved}`, path: saved }),
    );
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.metaKey || e.target instanceof HTMLInputElement) return;
      if (e.key === "c") {
        e.preventDefault();
        copy();
      } else if (e.key === "s") {
        e.preventDefault();
        void saveTo(e.shiftKey);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  return (
    <main className="bg-background text-foreground flex h-screen select-none">
      <section className="flex min-w-0 flex-1 flex-col">
        <header className="flex items-center gap-2 border-b px-4 py-2">
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium">{init?.name}</p>
            {init && (
              <p className="text-muted-foreground text-xs">
                {init.width} × {init.height} px
                {init.scaleFactor && init.scaleFactor > 1
                  ? ` · @${init.scaleFactor}x`
                  : ""}
              </p>
            )}
          </div>
          <Button
            size="sm"
            variant="outline"
            disabled={busy || !background}
            onClick={copy}
          >
            <Copy /> Copy
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={busy || !background}
            onClick={() => void saveTo(true)}
          >
            Save As…
          </Button>
          <Button
            size="sm"
            disabled={busy || !background}
            onClick={() => void saveTo(false)}
          >
            <Save /> Save
          </Button>
        </header>
        <div ref={area} className="min-h-0 flex-1 p-8">
          {preview && (
            <img
              src={preview}
              alt="Beautified screenshot"
              className="h-full w-full object-contain"
              draggable={false}
            />
          )}
        </div>
        {status && (
          <footer className="flex items-center gap-2 border-t px-4 py-2 text-sm">
            <span
              className={
                status.error ? "text-destructive truncate" : "truncate"
              }
              title={status.text}
            >
              {status.text}
            </span>
            {status.path && (
              <Button
                size="sm"
                variant="ghost"
                className="ml-auto shrink-0"
                onClick={() => commands.revealInFinder(status.path!)}
              >
                <FolderOpen /> Show in Finder
              </Button>
            )}
          </footer>
        )}
      </section>
      <aside className="w-72 shrink-0 overflow-y-auto border-l">
        {background && (
          <GestureContext.Provider value={NO_GESTURE}>
            <BackgroundControls
              background={background}
              onChange={(patch) =>
                setBackground((current) =>
                  current ? { ...current, ...patch } : current,
                )
              }
            />
          </GestureContext.Provider>
        )}
      </aside>
    </main>
  );
}
