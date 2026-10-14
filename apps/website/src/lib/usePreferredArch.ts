import { useEffect, useState } from "react";
import type { Arch } from "@/lib/release";

/** Intel Macs report an Intel GPU through WebGL; everything else gets the Apple Silicon build. */
export function usePreferredArch(): Arch {
  const [arch, setArch] = useState<Arch>("apple-silicon");

  useEffect(() => {
    const gl = document.createElement("canvas").getContext("webgl");
    const info = gl?.getExtension("WEBGL_debug_renderer_info");
    const renderer = info
      ? String(gl?.getParameter(info.UNMASKED_RENDERER_WEBGL))
      : "";
    if (/intel/i.test(renderer)) setArch("intel");
  }, []);

  return arch;
}
