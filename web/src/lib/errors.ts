import { ApiError } from "../api/client";
import type { T } from "../i18n";

/** A message for the person: the server's own words for 4xx, a generic line otherwise. */
export function errorText(err: unknown, t: T): string {
  if (err instanceof ApiError) {
    if (err.code === "network") return t("common.network");
    if (err.status >= 400 && err.status < 500) return capitalize(err.message);
  }
  return t("common.error");
}

function capitalize(s: string): string {
  return s ? s[0]!.toUpperCase() + s.slice(1) : s;
}
