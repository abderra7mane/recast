export const REPO_URL = "https://github.com/abderra7mane/recast";
export const RELEASES_URL = `${REPO_URL}/releases`;

export type Arch = "apple-silicon" | "intel" | "universal";

export const ARCHES: Record<
  Arch,
  { label: string; detail: string; suffix: string }
> = {
  "apple-silicon": {
    label: "Apple Silicon",
    detail: "M1 and later",
    suffix: "aarch64",
  },
  intel: { label: "Intel", detail: "Intel Macs", suffix: "x86_64" },
  universal: { label: "Universal", detail: "Any Mac", suffix: "universal" },
};

export const ARCH_ORDER: Arch[] = ["apple-silicon", "intel", "universal"];

interface GitHubRelease {
  assets: { name: string; browser_download_url: string }[];
}

const TTL_MS = 5 * 60 * 1000;
let cache: { at: number; value: GitHubRelease } | undefined;

export function downloadHref(arch: Arch) {
  return `/download/${arch}`;
}

async function fetchLatestRelease(): Promise<GitHubRelease> {
  if (cache && Date.now() - cache.at < TTL_MS) return cache.value;
  const response = await fetch(
    "https://api.github.com/repos/abderra7mane/recast/releases/latest",
    { headers: { Accept: "application/vnd.github+json" } },
  );
  if (!response.ok) throw new Error(`GitHub API: ${response.status}`);
  const value = (await response.json()) as GitHubRelease;
  cache = { at: Date.now(), value };
  return value;
}

/** The download URL of the latest release's installer for `arch`, if there is one. */
export async function latestDownloadUrl(arch: Arch): Promise<string | null> {
  const release = await fetchLatestRelease().catch(() => null);
  const asset = release?.assets.find((a) =>
    a.name.endsWith(`_${ARCHES[arch].suffix}.dmg`),
  );
  return asset?.browser_download_url ?? null;
}
