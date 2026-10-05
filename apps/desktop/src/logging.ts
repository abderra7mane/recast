import { error, info, warn } from "@tauri-apps/plugin-log";

const text = (args: unknown[]) =>
  args
    .map((arg) =>
      arg instanceof Error
        ? `${arg.message}\n${arg.stack ?? ""}`
        : typeof arg === "string"
          ? arg
          : JSON.stringify(arg),
    )
    .join(" ");

/** Sends console warnings, errors and uncaught failures to Recast's log files. */
export function forwardToLogFile(view: string) {
  if (!("__TAURI_INTERNALS__" in window)) return;
  const forward =
    (
      write: (message: string) => Promise<void>,
      original: typeof console.warn,
    ) =>
    (...args: unknown[]) => {
      original(...args);
      void write(`[${view}] ${text(args)}`).catch(() => {});
    };
  console.warn = forward(warn, console.warn.bind(console));
  console.error = forward(error, console.error.bind(console));
  window.addEventListener("error", (event) => {
    void error(
      `[${view}] ${event.message} at ${event.filename}:${event.lineno}`,
    ).catch(() => {});
  });
  window.addEventListener("unhandledrejection", (event) => {
    void error(`[${view}] unhandled rejection: ${text([event.reason])}`).catch(
      () => {},
    );
  });
  void info(`[${view}] opened`).catch(() => {});
}
