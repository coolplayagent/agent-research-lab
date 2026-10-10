export async function api<T>(
  path: string,
  body?: unknown,
  intent = "manage-crystal",
): Promise<T> {
  const response = await fetch(path, {
    method: body === undefined ? "GET" : "POST",
    cache: "no-store",
    signal: AbortSignal.timeout(15000),
    headers:
      body === undefined
        ? {}
        : { "Content-Type": "application/json", "X-Crystal-Intent": intent },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value.error || "请求失败，请重试。");
  return value as T;
}
export const change = <T = unknown>(body: unknown): Promise<T> =>
  api<T>("/api/crystal/manage", body);
export const query = (fields: Record<string, string | number>): string =>
  new URLSearchParams(
    Object.entries(fields).map(([k, v]) => [k, String(v)]),
  ).toString();
