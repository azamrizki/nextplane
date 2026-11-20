// The one way the web app talks to the server: JSON over fetch, same origin, cookie session.
// Errors come back as `{ error: { code, message } }` (SPEC §8) and are thrown as ApiError.

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

/** Identifies this tab, so realtime events it caused itself can be skipped. */
export const CLIENT_ID = Math.random().toString(36).slice(2) + Date.now().toString(36);

export async function api<T>(method: string, path: string, body?: unknown, raw?: { body: BodyInit; type: string }): Promise<T> {
  const init: RequestInit = {
    method,
    credentials: "same-origin",
    headers: { Accept: "application/json", "X-Client-Id": CLIENT_ID },
  };
  if (raw) {
    init.headers = { ...init.headers, "Content-Type": raw.type };
    init.body = raw.body;
  } else if (body !== undefined) {
    init.headers = { ...init.headers, "Content-Type": "application/json" };
    init.body = JSON.stringify(body);
  }
  let res: Response;
  try {
    res = await fetch(`/api/v1${path}`, init);
  } catch {
    throw new ApiError(0, "network", "network error");
  }
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  let data: unknown = undefined;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      data = undefined;
    }
  }
  if (!res.ok) {
    const err = (data as { error?: { code?: string; message?: string } } | undefined)?.error;
    throw new ApiError(res.status, err?.code ?? "http_" + res.status, err?.message ?? res.statusText);
  }
  return data as T;
}

export const get = <T>(path: string) => api<T>("GET", path);
export const post = <T>(path: string, body?: unknown) => api<T>("POST", path, body ?? {});
export const patch = <T>(path: string, body: unknown) => api<T>("PATCH", path, body);
export const put = <T>(path: string, body: unknown) => api<T>("PUT", path, body);
export const del = <T>(path: string) => api<T>("DELETE", path);
/** POST a text body (a CSV file, for example). */
export const postText = <T>(path: string, text: string, type = "text/csv") => api<T>("POST", path, undefined, { body: text, type });

/** GETs every page of a paged list (`X-Next-Cursor`), up to `max` items. */
export async function getAll<T>(path: string, max = 50_000): Promise<T[]> {
  const out: T[] = [];
  let cursor: string | null = null;
  do {
    const sep = path.includes("?") ? "&" : "?";
    const url: string = `/api/v1${path}${cursor ? `${sep}cursor=${encodeURIComponent(cursor)}` : ""}`;
    const res = await fetch(url, { credentials: "same-origin", headers: { Accept: "application/json", "X-Client-Id": CLIENT_ID } });
    if (!res.ok) {
      const data = await res.json().catch(() => undefined);
      throw new ApiError(res.status, data?.error?.code ?? "http_" + res.status, data?.error?.message ?? res.statusText);
    }
    out.push(...((await res.json()) as T[]));
    cursor = res.headers.get("x-next-cursor");
  } while (cursor && out.length < max);
  return out;
}
