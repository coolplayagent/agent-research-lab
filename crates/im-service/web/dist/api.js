import { t } from "./i18n.js";
export async function api(path, body, intent = "manage-crystal") {
    const response = await fetch(path, {
        method: body === undefined ? "GET" : "POST",
        cache: "no-store",
        signal: AbortSignal.timeout(15000),
        headers: body === undefined
            ? {}
            : { "Content-Type": "application/json", "X-Crystal-Intent": intent },
        body: body === undefined ? undefined : JSON.stringify(body),
    });
    const value = await response.json();
    if (!response.ok)
        throw new Error(value.error || t("api.a472d51792"));
    return value;
}
export const change = (body) => api("/api/crystal/manage", body);
export const query = (fields) => new URLSearchParams(Object.entries(fields).map(([k, v]) => [k, String(v)])).toString();
