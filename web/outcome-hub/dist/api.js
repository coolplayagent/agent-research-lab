import { t, describe } from "./i18n.js";
export class ApiError extends Error {
    status;
    constructor(message, status) {
        super(message);
        this.status = status;
    }
}
export async function api(path, body, intent = "manage-crystal", timeoutMs = 15000) {
    const response = await fetch(path, {
        method: body === undefined ? "GET" : "POST",
        cache: "no-store",
        signal: AbortSignal.timeout(timeoutMs),
        headers: body === undefined
            ? {}
            : { "Content-Type": "application/json", "X-Crystal-Intent": intent },
        body: body === undefined ? undefined : JSON.stringify(body),
    });
    const value = await response.json();
    if (!response.ok)
        throw new ApiError(describe(value.error) || t("api.a472d51792"), response.status);
    return value;
}
export const change = (body) => api("/api/v1/messaging/manage", body);
export const query = (fields) => new URLSearchParams(Object.entries(fields).map(([k, v]) => [k, String(v)])).toString();
