import { useSyncExternalStore } from "react";

const MOBILE_QUERY = "(max-width: 767px)";
const DESKTOP_VIEWPORT_WIDTH = 1280;

let headObserver: MutationObserver | null = null;
let cachedRawWidth = 0;

export function isMobileDesktopMode(): boolean {
  if (typeof window === "undefined" || typeof navigator === "undefined") {
    return false;
  }
  const ua = navigator.userAgent || "";
  const uaDataMobile = (navigator as Navigator & { userAgentData?: { mobile?: boolean } })
    .userAgentData?.mobile;
  const isMobileUa = uaDataMobile === true || /Mobi|iPhone|iPod|IEMobile|Opera Mini/i.test(ua);
  if (isMobileUa) {
    return false;
  }
  const isTouchDevice =
    (navigator.maxTouchPoints ?? 0) > 0 ||
    "ontouchstart" in window ||
    window.matchMedia?.("(pointer: coarse)")?.matches === true;
  const isCoarseOrNoHover =
    window.matchMedia?.("(hover: none), (pointer: coarse)")?.matches === true;
  const minScreen = Math.min(window.screen?.width || 0, window.screen?.height || 0);
  const hasMobileScreen = minScreen > 0 && minScreen < 1024;
  const isAndroidDesktopUa = /Android/i.test(ua) && !/Mobi/i.test(ua);

  return Boolean(isTouchDevice && (isCoarseOrNoHover || hasMobileScreen || isAndroidDesktopUa));
}

function getTargetViewportContent(): string {
  const currentWidth =
    typeof window !== "undefined"
      ? Math.min(
          window.innerWidth || DESKTOP_VIEWPORT_WIDTH,
          window.screen?.width || DESKTOP_VIEWPORT_WIDTH,
        )
      : DESKTOP_VIEWPORT_WIDTH;
  if (currentWidth > 0 && currentWidth < DESKTOP_VIEWPORT_WIDTH) {
    cachedRawWidth = currentWidth;
  }
  const rawWidth = cachedRawWidth;
  if (rawWidth > 0 && rawWidth < DESKTOP_VIEWPORT_WIDTH) {
    const scale = Number((rawWidth / DESKTOP_VIEWPORT_WIDTH).toFixed(4));
    return `width=${DESKTOP_VIEWPORT_WIDTH}, initial-scale=${scale}, minimum-scale=0.1, maximum-scale=5, user-scalable=yes`;
  }
  return `width=${DESKTOP_VIEWPORT_WIDTH}, minimum-scale=0.1, maximum-scale=5, user-scalable=yes`;
}

function applyViewportMeta(targetContent: string) {
  if (typeof document === "undefined" || !document.head) return;
  const metas = document.head.querySelectorAll?.('meta[name="viewport"]');
  if (metas && metas.length > 0) {
    const primary = metas[0];
    if (primary.getAttribute("content") !== targetContent) {
      primary.setAttribute("content", targetContent);
    }
    for (let i = 1; i < metas.length; i++) {
      metas[i].remove?.();
    }
  } else if (document.createElement) {
    const meta = document.createElement("meta");
    meta.setAttribute("name", "viewport");
    meta.setAttribute("content", targetContent);
    document.head.appendChild?.(meta);
  }
}

export function syncDesktopViewport(): boolean {
  if (typeof window === "undefined" || typeof document === "undefined") {
    return false;
  }
  const active = isMobileDesktopMode();
  const root = document.documentElement;
  if (!active) {
    if (root?.dataset && root.dataset.desktopViewport === "true") {
      delete root.dataset.desktopViewport;
      if (root.style) {
        root.style.zoom = "";
        root.style.minWidth = "";
      }
    }
    return false;
  }

  const targetContent = getTargetViewportContent();
  applyViewportMeta(targetContent);

  if (root) {
    if (root.dataset) {
      root.dataset.desktopViewport = "true";
    }
    if (root.style) {
      const layoutWidth = root.clientWidth || window.innerWidth || 0;
      if (layoutWidth > 0 && layoutWidth < 1024) {
        const zoom = Number((layoutWidth / DESKTOP_VIEWPORT_WIDTH).toFixed(4));
        if (zoom > 0 && zoom < 1) {
          root.style.zoom = String(zoom);
          root.style.minWidth = `${DESKTOP_VIEWPORT_WIDTH}px`;
        }
      }
    }
  }

  if (!headObserver && typeof MutationObserver !== "undefined" && document.head) {
    headObserver = new MutationObserver(() => {
      if (isMobileDesktopMode()) {
        applyViewportMeta(getTargetViewportContent());
      }
    });
    headObserver.observe(document.head, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["content"],
    });
  }

  return true;
}

if (typeof window !== "undefined") {
  syncDesktopViewport();
}

function subscribe(listener: () => void) {
  syncDesktopViewport();
  const media = window.matchMedia(MOBILE_QUERY);
  const handleChange = () => {
    cachedRawWidth = 0;
    syncDesktopViewport();
    listener();
  };
  media.addEventListener("change", handleChange);
  window.addEventListener?.("resize", handleChange);
  window.addEventListener?.("orientationchange", handleChange);
  return () => {
    media.removeEventListener("change", handleChange);
    window.removeEventListener?.("resize", handleChange);
    window.removeEventListener?.("orientationchange", handleChange);
  };
}

export function useIsMobile() {
  return useSyncExternalStore(
    subscribe,
    () => {
      if (syncDesktopViewport()) return false;
      return window.matchMedia(MOBILE_QUERY).matches;
    },
    () => false,
  );
}
