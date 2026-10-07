"use client";
import { createContext, useContext } from "react";

export const PlatformApiContext = createContext<"/consumers" | "/subscriptions">("/consumers");
export function usePlatformPrefix() {
  return useContext(PlatformApiContext);
}
