import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/")({ component: Home });

function Home() {
  return (
    <main className="flex min-h-dvh flex-col items-center justify-center gap-4 p-6 text-center font-sans">
      <img src="/icon-512.png" alt="" width={128} height={128} />
      <h1 className="text-4xl font-semibold">Recast</h1>
      <p className="text-lg text-neutral-600">Screen recorder for macOS</p>
      <p className="text-neutral-500">Coming soon</p>
      <a
        className="underline underline-offset-4"
        href="https://github.com/abderra7mane/recast"
      >
        GitHub
      </a>
    </main>
  );
}
