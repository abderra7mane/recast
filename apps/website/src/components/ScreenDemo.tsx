import { useEffect, useRef, useState } from "react";

const W = 1600;
const H = 1000;
const LOOP = 14;

interface Key {
  t: number;
  x: number;
  y: number;
  click?: boolean;
}

const PATH: Key[] = [
  { t: 0, x: 900, y: 720 },
  { t: 1.4, x: 196, y: 266, click: true },
  { t: 3.2, x: 640, y: 236, click: true },
  { t: 5.2, x: 980, y: 560 },
  { t: 6.6, x: 1452, y: 104, click: true },
  { t: 8.2, x: 1346, y: 268, click: true },
  { t: 10.6, x: 1040, y: 780 },
  { t: LOOP, x: 900, y: 720 },
];

const CLICKS = PATH.filter((k) => k.click).map((k) => k.t);

const ZOOMS = [
  { from: 0.8, to: 5.0, scale: 1.85 },
  { from: 6.0, to: 9.8, scale: 2.1 },
];

const TITLE = "Launch plan";

export type DemoMode = "raw" | "recast";

const BACKGROUND = "linear-gradient(135deg, #4f46e5, #db2777)";
const PADDING = 0.07;

const easeInOut = (x: number) =>
  x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2;
const clamp = (v: number, lo: number, hi: number) =>
  Math.min(hi, Math.max(lo, v));

function cursorAt(t: number, smooth: boolean) {
  const i = PATH.findIndex((k) => k.t > t);
  const a = PATH[Math.max(0, i - 1)];
  const b = PATH[i < 0 ? PATH.length - 1 : i];
  const span = b.t - a.t || 1;
  const p = clamp((t - a.t) / span, 0, 1);
  if (smooth) {
    const e = easeInOut(p);
    return { x: a.x + (b.x - a.x) * e, y: a.y + (b.y - a.y) * e };
  }
  const e = Math.min(1, p * 1.6);
  const jitter = (1 - e) * 14;
  return {
    x: a.x + (b.x - a.x) * e + Math.sin(t * 47) * jitter,
    y: a.y + (b.y - a.y) * e + Math.cos(t * 39) * jitter,
  };
}

function zoomAt(t: number) {
  for (const z of ZOOMS) {
    if (t < z.from || t > z.to) continue;
    const ramp = 0.7;
    const inP = clamp((t - z.from) / ramp, 0, 1);
    const outP = clamp((z.to - t) / ramp, 0, 1);
    return 1 + (z.scale - 1) * easeInOut(Math.min(inP, outP));
  }
  return 1;
}

function phaseAt(t: number) {
  const typed =
    t < 3.4 ? 0 : Math.min(TITLE.length, Math.floor((t - 3.4) / 0.12));
  return {
    newPage: t >= 1.4,
    typed,
    caret: t >= 3.2 && t < 6.6,
    popover: t >= 6.6 && t < 8.4,
    toast: t >= 8.2 && t < 11,
  };
}

export function ScreenDemo({
  mode,
  radius,
}: {
  mode: DemoMode;
  radius: number;
}) {
  const outer = useRef<HTMLDivElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const camera = useRef<HTMLDivElement>(null);
  const cursor = useRef<HTMLDivElement>(null);
  const ripple = useRef<HTMLDivElement>(null);
  const progress = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(0.5);
  const [phase, setPhase] = useState(() => phaseAt(2));
  const [playing, setPlaying] = useState(true);
  const recast = mode === "recast";
  const pad = recast ? PADDING : 0;

  useEffect(() => {
    const el = outer.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      setScale((entry.contentRect.width * (1 - 2 * pad)) / W);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [pad]);

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setPlaying(false);
    }
  }, []);

  useEffect(() => {
    const el = outer.current;
    if (!el || !playing) return;
    let frame = 0;
    let visible = true;
    let last = performance.now();
    let t = 0;
    let lastPhase = "";
    const io = new IntersectionObserver(([e]) => {
      visible = e.isIntersecting;
    });
    io.observe(el);

    const tick = (now: number) => {
      frame = requestAnimationFrame(tick);
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      if (!visible) return;
      t = (t + dt) % LOOP;

      const c = cursorAt(t, recast);
      const z = recast ? zoomAt(t) : 1;
      const fx = clamp(c.x, W / (2 * z), W - W / (2 * z));
      const fy = clamp(c.y, H / (2 * z), H - H / (2 * z));
      if (camera.current) {
        camera.current.style.transform = `translate(${W / 2}px, ${H / 2}px) scale(${z}) translate(${-fx}px, ${-fy}px)`;
      }
      if (cursor.current) {
        cursor.current.style.transform = `translate(${c.x}px, ${c.y}px) scale(${recast ? 1.35 : 1})`;
      }
      const lastClick = CLICKS.filter((ct) => ct <= t).pop();
      if (ripple.current) {
        const age = lastClick === undefined ? 1 : t - lastClick;
        const show = recast && age < 0.6;
        const k = PATH.find((p) => p.t === lastClick);
        ripple.current.style.opacity = show ? String(1 - age / 0.6) : "0";
        if (show && k) {
          ripple.current.style.transform = `translate(${k.x}px, ${k.y}px) scale(${0.3 + age * 2.2})`;
        }
      }
      if (progress.current) {
        progress.current.style.transform = `scaleX(${t / LOOP})`;
      }
      const p = phaseAt(t);
      const key = JSON.stringify(p);
      if (key !== lastPhase) {
        lastPhase = key;
        setPhase(p);
      }
    };
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      io.disconnect();
    };
  }, [playing, recast]);

  return (
    <div
      ref={outer}
      className="relative aspect-[16/10] w-full overflow-hidden select-none"
      style={{
        background: recast ? BACKGROUND : "#0b0b0f",
        borderRadius: radius * 1.4,
        transition: "background 400ms",
      }}
    >
      <div
        className="absolute overflow-hidden"
        style={{
          left: `${pad * 100}%`,
          right: `${pad * 100}%`,
          top: `${pad * 100}%`,
          bottom: `${pad * 100}%`,
          borderRadius: recast ? radius : 0,
          boxShadow: recast ? "0 30px 60px -12px rgba(0,0,0,.45)" : "none",
          transition: "all 400ms cubic-bezier(.2,.8,.2,1)",
        }}
      >
        <div
          ref={stage}
          style={{
            width: W,
            height: H,
            transform: `scale(${scale})`,
            transformOrigin: "0 0",
          }}
        >
          <div
            ref={camera}
            style={{ width: W, height: H, transformOrigin: "0 0" }}
          >
            <Scene phase={phase} />
            <div
              ref={ripple}
              className="pointer-events-none absolute top-0 left-0"
              style={{ opacity: 0 }}
            >
              <div className="-mt-[40px] -ml-[40px] size-[80px] rounded-full border-[5px] border-white/90 bg-white/25 shadow-[0_0_0_2px_rgba(79,70,229,.6)]" />
            </div>
            <div
              ref={cursor}
              className="pointer-events-none absolute top-0 left-0"
              style={{ transformOrigin: "0 0" }}
            >
              <Pointer />
            </div>
          </div>
        </div>
      </div>
      <div className="absolute inset-x-0 bottom-0 h-[3px] bg-white/10">
        <div
          ref={progress}
          className="h-full origin-left bg-white/70"
          style={{ transform: "scaleX(0)" }}
        />
      </div>
      <button
        type="button"
        onClick={() => setPlaying((p) => !p)}
        className="absolute right-3 bottom-3 grid size-8 place-items-center rounded-full bg-black/40 text-white backdrop-blur transition hover:bg-black/60"
        aria-label={playing ? "Pause demo" : "Play demo"}
      >
        {playing ? (
          <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor">
            <rect x="2" y="1" width="3" height="10" rx="1" />
            <rect x="7" y="1" width="3" height="10" rx="1" />
          </svg>
        ) : (
          <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor">
            <path d="M3 1.5v9l7.5-4.5z" />
          </svg>
        )}
      </button>
    </div>
  );
}

function Pointer({ size = 34 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size * 1.4}
      viewBox="0 0 20 28"
      style={{ filter: "drop-shadow(0 2px 3px rgba(0,0,0,.35))" }}
    >
      <path
        d="M1.5 1.5v21.2l5.4-5.2 3.4 8.1 3.6-1.5-3.4-8h7.6z"
        fill="#111"
        stroke="#fff"
        strokeWidth="1.6"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function Scene({ phase }: { phase: ReturnType<typeof phaseAt> }) {
  const pages = ["Roadmap", "Weekly sync", "Design review", "Hiring"];
  return (
    <div
      className="absolute inset-0 flex bg-[#f4f4f6] text-[#1d1d22]"
      style={{
        fontFamily: "-apple-system, BlinkMacSystemFont, 'Inter', sans-serif",
      }}
    >
      <aside className="flex w-[300px] flex-col gap-2 border-r border-black/8 bg-[#ebebef] px-5 pt-6">
        <div className="mb-6 flex gap-2.5 px-1">
          <span className="size-[18px] rounded-full bg-[#ff5f57]" />
          <span className="size-[18px] rounded-full bg-[#febc2e]" />
          <span className="size-[18px] rounded-full bg-[#28c840]" />
        </div>
        <div className="px-3 text-[17px] font-semibold tracking-wide text-black/40 uppercase">
          Pages
        </div>
        <div
          className={`flex h-[58px] items-center gap-3 rounded-xl px-4 text-[22px] font-medium transition ${phase.newPage ? "bg-white shadow-sm" : "text-black/55"}`}
        >
          <span className="text-[26px] text-[#4f46e5]">+</span>
          {phase.newPage
            ? phase.typed
              ? TITLE.slice(0, phase.typed)
              : "Untitled"
            : "New page"}
        </div>
        {pages.map((p) => (
          <div
            key={p}
            className="flex h-[54px] items-center gap-3 rounded-xl px-4 text-[21px] text-black/60"
          >
            <span className="size-[14px] rounded-[4px] bg-black/15" />
            {p}
          </div>
        ))}
      </aside>
      <main className="relative flex-1">
        <header className="flex h-[96px] items-center justify-between border-b border-black/8 px-10">
          <div className="flex gap-3">
            <span className="h-[14px] w-[120px] rounded-full bg-black/10" />
            <span className="h-[14px] w-[80px] rounded-full bg-black/10" />
          </div>
          <div className="flex items-center gap-4">
            <span className="size-[44px] rounded-full bg-gradient-to-br from-amber-300 to-rose-400" />
            <span
              className={`rounded-xl px-6 py-3 text-[21px] font-semibold text-white transition ${phase.popover ? "bg-[#3730a3]" : "bg-[#4f46e5]"}`}
            >
              Share
            </span>
          </div>
        </header>
        <div className="px-16 pt-14">
          <div className="flex h-[74px] items-center text-[52px] font-bold tracking-tight">
            {phase.newPage ? (
              phase.typed ? (
                TITLE.slice(0, phase.typed)
              ) : (
                <span className="text-black/25">Untitled</span>
              )
            ) : (
              "Roadmap"
            )}
            {phase.caret && (
              <span className="ml-1 inline-block h-[56px] w-[3px] animate-pulse bg-[#4f46e5]" />
            )}
          </div>
          <div className="mt-8 grid grid-cols-3 gap-6">
            {[
              ["#e0e7ff", "#4f46e5"],
              ["#fce7f3", "#db2777"],
              ["#dcfce7", "#16a34a"],
            ].map(([bg, fg], i) => (
              <div
                key={i}
                className="h-[260px] rounded-2xl border border-black/6 bg-white p-6 shadow-sm"
              >
                <span
                  className="inline-block rounded-lg px-3 py-1 text-[17px] font-semibold"
                  style={{ background: bg, color: fg }}
                >
                  {["Now", "Next", "Later"][i]}
                </span>
                <div className="mt-6 space-y-3">
                  <div className="h-[14px] w-[85%] rounded-full bg-black/10" />
                  <div className="h-[14px] w-[65%] rounded-full bg-black/10" />
                  <div className="h-[14px] w-[75%] rounded-full bg-black/8" />
                </div>
              </div>
            ))}
          </div>
          <div className="mt-8 space-y-4">
            <div className="h-[16px] w-[70%] rounded-full bg-black/8" />
            <div className="h-[16px] w-[55%] rounded-full bg-black/8" />
            <div className="h-[16px] w-[62%] rounded-full bg-black/8" />
          </div>
        </div>
        {phase.popover && (
          <div className="absolute top-[118px] right-[40px] w-[380px] rounded-2xl border border-black/8 bg-white p-3 shadow-2xl">
            <div className="flex h-[60px] items-center rounded-xl bg-[#eef2ff] px-4 text-[21px] font-medium text-[#3730a3]">
              Copy link
            </div>
            <div className="flex h-[60px] items-center px-4 text-[21px] text-black/60">
              Invite people
            </div>
            <div className="flex h-[60px] items-center px-4 text-[21px] text-black/60">
              Publish to web
            </div>
          </div>
        )}
        {phase.toast && (
          <div className="absolute bottom-[48px] left-1/2 -translate-x-1/2 rounded-full bg-[#1d1d22] px-8 py-4 text-[21px] font-medium text-white shadow-xl">
            Link copied
          </div>
        )}
      </main>
    </div>
  );
}
