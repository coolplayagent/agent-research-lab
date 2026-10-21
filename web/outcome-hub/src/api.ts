import { t, describe } from "./i18n.js";
export class ApiError extends Error {
  constructor(message: string, public readonly status: number) { super(message); }
}
export async function api<T>(
  path: string,
  body?: unknown,
  intent = "manage-crystal",
  timeoutMs = 15000,
): Promise<T> {
  const response = await fetch(path, {
    method: body === undefined ? "GET" : "POST",
    cache: "no-store",
    signal: AbortSignal.timeout(timeoutMs),
    headers:
      body === undefined
        ? {}
        : { "Content-Type": "application/json", "X-Crystal-Intent": intent },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok) throw new ApiError(describe(value.error) || t("api.a472d51792"), response.status);
  return value as T;
}
export const change = <T = unknown>(body: unknown): Promise<T> =>
  api<T>("/api/v1/messaging/manage", body);
export const query = (fields: Record<string, string | number>): string =>
  new URLSearchParams(
    Object.entries(fields).map(([k, v]) => [k, String(v)]),
  ).toString();
