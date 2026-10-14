import { type ReactNode, useEffect, useState } from "react";

type Theme = "system" | "light" | "dark";

const KEY = "theme";

/** Runs in <head> before the first paint, so a stored choice never flashes the other theme. */
export const THEME_SCRIPT = `try{var t=localStorage.getItem("${KEY}");if(t==="light"||t==="dark")document.documentElement.classList.add(t)}catch(e){}`;

const OPTIONS: { value: Theme; label: string; icon: ReactNode }[] = [
  {
    value: "system",
    label: "System theme",
    icon: (
      <>
        <rect x="2" y="3" width="12" height="8" rx="1.5" />
        <path d="M6 14h4M8 11v3" />
      </>
    ),
  },
  {
    value: "light",
    label: "Light theme",
    icon: (
      <>
        <circle cx="8" cy="8" r="3" />
        <path d="M8 1.5v1.5M8 13v1.5M1.5 8H3M13 8h1.5M3.4 3.4l1 1M11.6 11.6l1 1M3.4 12.6l1-1M11.6 4.4l1-1" />
      </>
    ),
  },
  {
    value: "dark",
    label: "Dark theme",
    icon: <path d="M13.5 9.5A5.5 5.5 0 0 1 6.5 2.5a5.5 5.5 0 1 0 7 7z" />,
  },
];

export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>("system");

  useEffect(() => {
    const root = document.documentElement;
    if (root.classList.contains("light")) setTheme("light");
    else if (root.classList.contains("dark")) setTheme("dark");
  }, []);

  const choose = (next: Theme) => {
    setTheme(next);
    const root = document.documentElement;
    root.classList.remove("light", "dark");
    if (next !== "system") root.classList.add(next);
    try {
      if (next === "system") localStorage.removeItem(KEY);
      else localStorage.setItem(KEY, next);
    } catch {
      // Storage can be blocked; the choice still applies to this page.
    }
  };

  return (
    <div
      role="radiogroup"
      aria-label="Theme"
      className="flex rounded-full border border-fg/10 p-0.5"
    >
      {OPTIONS.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={theme === o.value}
          aria-label={o.label}
          title={o.label}
          onClick={() => choose(o.value)}
          className={`grid size-7 place-items-center rounded-full transition ${theme === o.value ? "bg-fg/10 text-fg" : "text-fg/45 hover:text-fg"}`}
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            {o.icon}
          </svg>
        </button>
      ))}
    </div>
  );
}
