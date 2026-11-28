import { createSignal, onCleanup } from "solid-js";
import type { ImageSize } from "../../types";

export function imageSize(value: unknown): ImageSize | undefined {
  if (!value || typeof value !== "object") return;
  const { width, height } = value as ImageSize;
  if (Number.isSafeInteger(width) && Number.isSafeInteger(height) && width > 0 && height > 0 && width * height <= 64_000_000) return { width, height };
}

/** Keep Qt's 96×64 cell; the image's final fitted box is known before decode. */
export function fittedImage(size: ImageSize | undefined) {
  if (!size) return { width: "96px", height: "64px" };
  const scale = Math.min(1, 96 / size.width, 64 / size.height);
  return { width: `${size.width * scale}px`, height: `${size.height * scale}px` };
}

/** Publish only decoded sources. A failed recorded copy never replaces a
 * working local preview, even when both copies use the same cache URL. */
export function createImageHandoff(decode: (url: string) => Promise<void>) {
  const [url, setUrl] = createSignal<string>();
  let localRevision = 0, remoteRevision = 0;
  onCleanup(() => { localRevision++; remoteRevision++; });
  return {
    url,
    async local(value: string | undefined) {
      const ticket = ++localRevision;
      if (!value || url()) return;
      try {
        await decode(value);
        if (ticket === localRevision && !url()) setUrl(value);
      } catch { /* The guarded history read may restore this same URL. */ }
    },
    async remote(value: string | undefined, failed?: () => void) {
      const ticket = ++remoteRevision;
      if (!value || value === url()) return;
      try {
        await decode(value);
        if (ticket === remoteRevision) setUrl(value);
      } catch { if (ticket === remoteRevision) failed?.(); }
    },
    broken(value: string, current: () => boolean): boolean {
      if (url() !== value || !current()) return false;
      setUrl(undefined);
      return true;
    },
  };
}
