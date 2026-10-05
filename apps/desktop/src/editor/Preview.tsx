import { useEffect, useRef, useState } from "react";

import { commands } from "@/bindings";
import { parseFrame, type Frame } from "@/editor/frame";
import { frameDrawn, playbackStore } from "@/editor/playback";
import { createRenderer, type RendererKind } from "@/editor/renderer";

const RECONNECT_MS = 500;

function preferredRenderer(): RendererKind | "auto" {
  try {
    const value = localStorage.getItem("recast.renderer");
    return value === "webgl2" || value === "webgpu" ? value : "auto";
  } catch {
    return "auto";
  }
}

/** Largest box of `aspect` that fits in `width × height`. */
function fit(width: number, height: number, aspect: number) {
  return width / height > aspect
    ? { width: height * aspect, height }
    : { width, height: width / aspect };
}

export function Preview({ url }: { url: string }) {
  const areaRef = useRef<HTMLDivElement>(null);
  const hostRef = useRef<HTMLDivElement>(null);
  const latest = useRef<Frame | null>(null);
  const [area, setArea] = useState({ width: 0, height: 0 });
  const [aspect, setAspect] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const element = areaRef.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setArea({ width, height });
      const dpr = window.devicePixelRatio || 1;
      void commands.editorResize(
        Math.round(width * dpr),
        Math.round(height * dpr),
      );
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    let socket: WebSocket | null = null;
    let retry: number | undefined;
    let closed = false;
    const connect = () => {
      socket = new WebSocket(url);
      socket.binaryType = "arraybuffer";
      socket.onopen = () => playbackStore.setState({ connected: true });
      socket.onmessage = (event) => {
        const frame = parseFrame(event.data as ArrayBuffer);
        if (frame && frame.header.seq > (latest.current?.header.seq ?? -1)) {
          latest.current = frame;
        }
      };
      socket.onclose = () => {
        if (closed) return;
        playbackStore.setState({ connected: false });
        latest.current = null;
        if (!closed) retry = window.setTimeout(connect, RECONNECT_MS);
      };
    };
    connect();
    return () => {
      closed = true;
      window.clearTimeout(retry);
      socket?.close();
    };
  }, [url]);

  useEffect(() => {
    let frameRequest = 0;
    let cancelled = false;
    let drawnSeq = -1;
    let drawn: number[] = [];
    let renderer: Awaited<ReturnType<typeof createRenderer>> | null = null;

    void createRenderer(preferredRenderer())
      .then((created) => {
        if (cancelled) {
          created.destroy();
          return;
        }
        renderer = created;
        created.canvas.className = "block h-full w-full";
        hostRef.current?.appendChild(created.canvas);
        playbackStore.setState({ renderer: created.kind });
      })
      .catch((e: unknown) => setError(String(e)));

    const redraw = () => {
      if (document.visibilityState === "visible") drawnSeq = -1;
    };
    document.addEventListener("visibilitychange", redraw);
    window.addEventListener("focus", redraw);

    const tick = () => {
      frameRequest = requestAnimationFrame(tick);
      const frame = latest.current;
      if (!renderer || !frame || frame.header.seq === drawnSeq) return;
      drawnSeq = frame.header.seq;
      renderer.draw(frame);
      frameDrawn(frame.header);
      const now = performance.now();
      drawn = drawn.filter((t) => now - t < 1000);
      drawn.push(now);
      playbackStore.setState({ fps: drawn.length });
      const next = frame.header.width / frame.header.height;
      setAspect((current) =>
        current !== null && Math.abs(current - next) < 1e-4 ? current : next,
      );
    };
    frameRequest = requestAnimationFrame(tick);

    return () => {
      cancelled = true;
      document.removeEventListener("visibilitychange", redraw);
      window.removeEventListener("focus", redraw);
      cancelAnimationFrame(frameRequest);
      renderer?.canvas.remove();
      renderer?.destroy();
    };
  }, []);

  const size =
    aspect && area.width > 0 ? fit(area.width, area.height, aspect) : null;

  return (
    <div ref={areaRef} className="relative h-full w-full">
      <div
        ref={hostRef}
        className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-sm shadow-2xl"
        style={
          size
            ? { width: size.width, height: size.height }
            : { visibility: "hidden" }
        }
      />
      {error && (
        <p className="text-destructive absolute inset-0 flex items-center justify-center text-sm">
          {error}
        </p>
      )}
    </div>
  );
}
