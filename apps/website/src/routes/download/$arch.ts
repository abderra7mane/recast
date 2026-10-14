import { createFileRoute } from "@tanstack/react-router";
import {
  ARCHES,
  type Arch,
  RELEASES_URL,
  latestDownloadUrl,
} from "@/lib/release";

export const Route = createFileRoute("/download/$arch")({
  server: {
    handlers: {
      GET: async ({ params }) => {
        const arch = params.arch as Arch;
        if (!(arch in ARCHES))
          return new Response("Not found", { status: 404 });
        const location =
          (await latestDownloadUrl(arch)) ?? `${RELEASES_URL}/latest`;
        return new Response(null, {
          status: 302,
          headers: {
            Location: location,
            "Cache-Control": "public, max-age=0, s-maxage=300",
          },
        });
      },
    },
  },
});
