import {
  type MouseEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import { type DemoMode, ScreenDemo } from "@/components/ScreenDemo";
import { ThemeToggle } from "@/components/ThemeToggle";
import {
  FEATURES,
  INSTALL_STEPS,
  PERMISSIONS,
  REQUIREMENTS,
  SHORTCUTS,
} from "@/lib/content";
import {
  ARCHES,
  ARCH_ORDER,
  type Arch,
  REPO_URL,
  downloadHref,
} from "@/lib/release";
import { usePreferredArch } from "@/lib/usePreferredArch";

const ICONS: Record<string, ReactNode> = {
  record: <circle cx="12" cy="12" r="5" fill="currentColor" />,
  edit: <path d="M4 17h4l9-9-4-4-9 9zM13 4l4 4" strokeLinejoin="round" />,
  export: <path d="M12 4v11M7 10l5 5 5-5M5 20h14" strokeLinecap="round" />,
  capture: (
    <path d="M4 8V5a1 1 0 0 1 1-1h3M16 4h3a1 1 0 0 1 1 1v3M20 16v3a1 1 0 0 1-1 1h-3M8 20H5a1 1 0 0 1-1-1v-3" />
  ),
  library: <path d="M4 6h16M4 12h16M4 18h10" strokeLinecap="round" />,
  shortcuts: (
    <path d="M7 9a2 2 0 1 1 2-2v10a2 2 0 1 1-2-2h10a2 2 0 1 1-2 2V7a2 2 0 1 1 2 2z" />
  ),
};

const PIPELINE = [
  [
    "01",
    "Capture",
    "ScreenCaptureKit writes raw HEVC video and a log of every click and cursor move.",
  ],
  [
    "02",
    "Compose",
    "One GPU compositor renders zoom, cursor, click effects and background, live in the editor.",
  ],
  [
    "03",
    "Export",
    "The same compositor renders the final MP4, in H.264 or HEVC, up to 4K.",
  ],
];

export function Home() {
  const preferred = usePreferredArch();
  const [mode, setMode] = useState<DemoMode>("recast");

  return (
    <div
      className="relative min-h-dvh overflow-x-clip bg-bg text-fg antialiased"
      style={{ fontFamily: "'Inter Tight', Inter, -apple-system, sans-serif" }}
    >
      <div className="pointer-events-none absolute inset-x-0 top-0 h-[900px] bg-[radial-gradient(60%_50%_at_50%_0%,var(--glow),transparent_70%)]" />

      <header className="relative z-30 mx-auto flex max-w-6xl items-center justify-between gap-3 px-6 py-5">
        <a
          href="#"
          className="flex items-center gap-2.5 font-semibold tracking-tight"
        >
          <img src="/icon-512.png" alt="" className="size-7" />
          Recast
        </a>
        <nav className="flex items-center gap-1 text-sm text-fg/60">
          <a
            href="#features"
            className="hidden rounded-full px-3 py-1.5 hover:text-fg sm:block"
          >
            Features
          </a>
          <a
            href="#download"
            className="hidden rounded-full px-3 py-1.5 hover:text-fg sm:block"
          >
            Install
          </a>
          <a
            href={REPO_URL}
            className="hidden rounded-full px-3 py-1.5 hover:text-fg sm:block"
          >
            GitHub
          </a>
          <ThemeToggle />
          <a
            href={downloadHref(preferred)}
            className="ml-1 rounded-full bg-fg px-4 py-1.5 font-medium text-bg transition hover:bg-fg/85"
          >
            Download
          </a>
        </nav>
      </header>

      <section className="relative z-20 mx-auto max-w-6xl px-6 pt-16 text-center sm:pt-24">
        <a
          href={REPO_URL}
          className="inline-flex items-center gap-2 rounded-full border border-fg/10 bg-fg/[.04] py-1 pr-3 pl-1 text-xs text-fg/70 transition hover:border-fg/25"
        >
          <span className="rounded-full bg-[#ea4440] px-2 py-0.5 font-semibold text-white">
            MIT
          </span>
          Free and open source
          <span aria-hidden>→</span>
        </a>
        <h1 className="mx-auto mt-7 max-w-4xl text-[clamp(2.6rem,7vw,5.6rem)] leading-[0.95] font-semibold tracking-[-0.045em]">
          Screen recordings
          <br />
          <span className="bg-[linear-gradient(90deg,var(--title-from),var(--title-via),var(--title-to))] bg-clip-text text-transparent">
            that direct themselves.
          </span>
        </h1>
        <p className="mx-auto mt-6 max-w-xl text-lg text-fg/55">
          Recast zooms in on every click, smooths the cursor and frames it all
          in a background. Applied when you export, never baked in.
        </p>
        <div className="mt-9 flex flex-wrap items-center justify-center gap-3">
          <DownloadButton preferred={preferred} />
          <a
            href={REPO_URL}
            className="rounded-full border border-fg/15 px-6 py-3 font-medium text-fg/85 transition hover:border-fg/35 hover:text-fg"
          >
            View source
          </a>
        </div>
        <p className="mt-4 text-xs text-fg/40">
          {REQUIREMENTS} · Apple Silicon and Intel
        </p>
      </section>

      <section className="relative z-10 mx-auto mt-16 max-w-5xl px-6">
        <div
          className="absolute inset-x-10 -inset-y-6 rounded-[48px] bg-[conic-gradient(from_180deg_at_50%_50%,#4f46e5,#db2777,#ea4440,#4f46e5)] blur-[90px]"
          style={{ opacity: "var(--halo)" }}
        />
        <div
          className="relative rounded-[28px] border border-fg/10 bg-fg/[.03] p-2"
          style={{ boxShadow: "var(--frame-shadow)" }}
        >
          <ScreenDemo mode={mode} radius={16} />
        </div>
        <div className="relative mt-6 flex justify-center">
          <div className="inline-flex rounded-full border border-fg/10 bg-fg/[.04] p-1 text-sm">
            {(["raw", "recast"] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => setMode(m)}
                className={`rounded-full px-5 py-2 transition ${mode === m ? "bg-fg text-bg" : "text-fg/55 hover:text-fg"}`}
              >
                {m === "raw" ? "Raw recording" : "With Recast"}
              </button>
            ))}
          </div>
        </div>
      </section>

      <section className="relative mx-auto mt-36 max-w-6xl px-6">
        <p className="text-sm font-medium tracking-[0.2em] text-accent uppercase">
          Never baked in
        </p>
        <h2 className="mt-3 max-w-2xl text-4xl font-semibold tracking-[-0.03em] sm:text-5xl">
          Record the raw take. Decide how it looks later.
        </h2>
        <div className="mt-14 grid gap-px overflow-hidden rounded-3xl border border-fg/10 bg-fg/10 md:grid-cols-3">
          {PIPELINE.map(([n, title, body]) => (
            <div key={n} className="bg-bg p-8">
              <div className="font-mono text-xs text-fg/35">{n}</div>
              <h3 className="mt-8 text-xl font-semibold">{title}</h3>
              <p className="mt-2 text-fg/55">{body}</p>
            </div>
          ))}
        </div>
      </section>

      <section id="features" className="relative mx-auto mt-36 max-w-6xl px-6">
        <h2 className="max-w-xl text-4xl font-semibold tracking-[-0.03em] sm:text-5xl">
          Everything lives in your menu bar.
        </h2>
        <div className="mt-12 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {FEATURES.map((f) => (
            <GlowCard key={f.id}>
              <span className="grid size-10 place-items-center rounded-xl bg-fg/[.06] text-accent">
                <svg
                  width="22"
                  height="22"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.7"
                >
                  {ICONS[f.id]}
                </svg>
              </span>
              <h3 className="mt-6 text-lg font-semibold">{f.title}</h3>
              <p className="mt-2 text-[15px] leading-relaxed text-fg/55">
                {f.body}
              </p>
            </GlowCard>
          ))}
        </div>
      </section>

      <section className="relative mx-auto mt-36 max-w-6xl px-6">
        <div className="grid items-center gap-10 rounded-[32px] border border-fg/10 bg-gradient-to-br from-fg/[.06] to-transparent p-10 md:grid-cols-2 md:p-14">
          <div>
            <h2 className="text-3xl font-semibold tracking-[-0.03em] sm:text-4xl">
              One shortcut, from anywhere.
            </h2>
            <p className="mt-4 text-fg/55">
              Every record and capture mode can have a global shortcut. Record
              and capture an area are set up for you; change any of them in
              Settings.
            </p>
          </div>
          <div className="space-y-6">
            {SHORTCUTS.map((s) => (
              <div
                key={s.action}
                className="flex flex-wrap items-center justify-between gap-4"
              >
                <span className="text-fg/60">{s.action}</span>
                <span className="flex gap-2">
                  {s.keys.map((k) => (
                    <kbd
                      key={k}
                      className="grid h-14 min-w-14 place-items-center rounded-xl border border-fg/15 bg-[linear-gradient(to_bottom,var(--key-from),var(--key-to))] px-3 font-sans text-xl"
                      style={{ boxShadow: "var(--key-shadow)" }}
                    >
                      {k}
                    </kbd>
                  ))}
                </span>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section
        id="download"
        className="relative mx-auto mt-36 max-w-6xl px-6 pb-32"
      >
        <div className="text-center">
          <img
            src="/icon-512.png"
            alt=""
            className="mx-auto size-24 drop-shadow-[0_20px_40px_var(--icon-shadow)]"
          />
          <h2 className="mt-6 text-4xl font-semibold tracking-[-0.03em] sm:text-5xl">
            Get Recast
          </h2>
          <p className="mt-3 text-fg/55">
            {REQUIREMENTS}. Free, open source, and it updates itself.
          </p>
        </div>
        <div className="mt-12 grid gap-4 md:grid-cols-3">
          {ARCH_ORDER.map((arch) => (
            <a
              key={arch}
              href={downloadHref(arch)}
              className={`group relative rounded-3xl border p-7 transition ${arch === preferred ? "border-accent/60 bg-accent/[.08]" : "border-fg/10 bg-fg/[.03] hover:border-fg/25"}`}
            >
              {arch === preferred && (
                <span className="absolute top-6 right-6 rounded-full bg-accent px-2.5 py-0.5 text-[11px] font-semibold text-bg">
                  Your Mac
                </span>
              )}
              <div className="text-xl font-semibold">{ARCHES[arch].label}</div>
              <div className="text-sm text-fg/50">{ARCHES[arch].detail}</div>
              <div className="mt-10 flex justify-end">
                <span className="grid size-10 place-items-center rounded-full bg-fg text-bg transition group-hover:scale-110">
                  <DownloadGlyph />
                </span>
              </div>
            </a>
          ))}
        </div>
        <div className="mt-6 grid gap-4 md:grid-cols-[1.4fr_1fr]">
          <div className="rounded-3xl border border-fg/10 p-7">
            <h3 className="font-semibold">First launch</h3>
            <ol className="mt-4 space-y-3">
              {INSTALL_STEPS.map((step, i) => (
                <li key={step} className="flex gap-3 text-[15px] text-fg/60">
                  <span className="grid size-6 shrink-0 place-items-center rounded-full bg-fg/10 text-xs text-fg">
                    {i + 1}
                  </span>
                  {step}
                </li>
              ))}
            </ol>
          </div>
          <div className="rounded-3xl border border-fg/10 p-7">
            <h3 className="font-semibold">Permissions</h3>
            <ul className="mt-4 space-y-3">
              {PERMISSIONS.map((p) => (
                <li key={p.name} className="text-[15px]">
                  <span className="text-fg/85">{p.name}</span>
                  <span className="text-fg/50">, {p.why}</span>
                </li>
              ))}
            </ul>
          </div>
        </div>
      </section>

      <footer className="relative border-t border-fg/10">
        <div className="mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-4 px-6 py-8 text-sm text-fg/45">
          <span className="flex items-center gap-2">
            <img src="/icon-512.png" alt="" className="size-5" /> Recast · MIT
            License
          </span>
          <a href={REPO_URL} className="hover:text-fg">
            github.com/abderra7mane/recast
          </a>
        </div>
      </footer>
    </div>
  );
}

function DownloadButton({ preferred }: { preferred: Arch }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={root} className="relative inline-flex">
      <a
        href={downloadHref(preferred)}
        className="flex items-center gap-2.5 rounded-l-full bg-fg py-3 whitespace-nowrap pr-5 pl-6 font-medium text-bg transition hover:bg-fg/90"
      >
        <DownloadGlyph />
        Download for Mac
        <span className="hidden text-bg/55 sm:inline">
          {ARCHES[preferred].label}
        </span>
      </a>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="rounded-r-full border-l border-bg/15 bg-fg px-3.5 text-bg/70 transition hover:bg-fg/90"
        aria-label="Other builds"
        aria-haspopup="menu"
        aria-expanded={open}
      >
        <svg
          width="12"
          height="12"
          viewBox="0 0 12 12"
          fill="currentColor"
          className={`transition ${open ? "rotate-180" : ""}`}
        >
          <path d="M2.5 4.5 6 8l3.5-3.5z" />
        </svg>
      </button>
      {open && (
        <div
          role="menu"
          className="absolute inset-x-0 top-full z-30 mt-2 rounded-2xl border border-fg/10 bg-popover p-1.5 text-left shadow-[0_20px_50px_-12px_rgba(0,0,0,.45)]"
        >
          {ARCH_ORDER.map((arch) => (
            <a
              key={arch}
              role="menuitem"
              href={downloadHref(arch)}
              onClick={() => setOpen(false)}
              className="block rounded-xl px-3 py-2.5 text-sm hover:bg-fg/[.06]"
            >
              {ARCHES[arch].label}
              <span className="block text-xs text-fg/45">
                {ARCHES[arch].detail}
              </span>
            </a>
          ))}
        </div>
      )}
    </div>
  );
}

function GlowCard({ children }: { children: ReactNode }) {
  const onMove = (e: MouseEvent<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    e.currentTarget.style.setProperty("--x", `${e.clientX - r.left}px`);
    e.currentTarget.style.setProperty("--y", `${e.clientY - r.top}px`);
  };
  return (
    <div
      onMouseMove={onMove}
      className="group relative overflow-hidden rounded-3xl border border-fg/10 bg-fg/[.025] p-7 transition hover:border-fg/20"
    >
      <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(400px_circle_at_var(--x)_var(--y),var(--card-glow),transparent_60%)] opacity-0 transition group-hover:opacity-100" />
      <div className="relative">{children}</div>
    </div>
  );
}

function DownloadGlyph() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
    >
      <path d="M8 2v9M4 7.5 8 11.5l4-4M3 14h10" />
    </svg>
  );
}
