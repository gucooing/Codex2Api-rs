// Renew only while this page is visible; returning to it also checks the session.
export function keepSessionAlive(
  refresh: (signal: AbortSignal) => Promise<unknown>,
  onError: (error: unknown) => void,
) {
  const controller = new AbortController();
  const interval = 10 * 60 * 1000;
  let refreshedAt = -Infinity;
  let pending = false;
  const renew = async () => {
    if (
      controller.signal.aborted ||
      document.visibilityState !== "visible" ||
      pending ||
      Date.now() - refreshedAt < interval
    )
      return;
    pending = true;
    try {
      await refresh(controller.signal);
      refreshedAt = Date.now();
    } catch (error) {
      if (!controller.signal.aborted) onError(error);
    } finally {
      pending = false;
    }
  };
  const check = () => void renew();
  const timer = window.setInterval(check, interval);
  document.addEventListener("visibilitychange", check);
  window.addEventListener("focus", check);
  window.addEventListener("pageshow", check);
  window.addEventListener("online", check);
  check();
  return () => {
    controller.abort();
    window.clearInterval(timer);
    document.removeEventListener("visibilitychange", check);
    window.removeEventListener("focus", check);
    window.removeEventListener("pageshow", check);
    window.removeEventListener("online", check);
  };
}
