"use client";
import { createContext, useContext } from "react";

export const PlatformApiContext = createContext<"/subscriptions">("/subscriptions");
export function usePlatformPrefix() {
  return useContext(PlatformApiContext);
}
