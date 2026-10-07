"use client";
import { useEffect, useState } from "react";

/** A local clock tick never performs network I/O. */
export function useQuotaClock() {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30000);
    return () => window.clearInterval(timer);
  }, []);
  return now;
}
