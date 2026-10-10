import { t } from "./i18n.js";
import { label } from "./i18n.js";
export const $ = (id) => document.getElementById(id);
export const el = (tag, text, cls) => {
    const n = document.createElement(tag);
    if (text !== undefined)
        n.textContent = text;
    if (cls)
        n.className = cls;
    return n;
};
export const badge = (state) => el("span", label("ExecutionState", state) || state || t("ui.4d8c1c5b42"), `badge ${state || "unknown"}`);
export const short = (s, n = 10) => (s || "").slice(0, n);
export const time = (t) => t ? new Date(t * 1000).toLocaleTimeString("zh-CN", { hour12: false }) : "—";
export function messageTimestamp(seconds) {
    const date = new Date(seconds * 1000);
    if (typeof seconds !== "number" || !Number.isFinite(date.getTime()))
        return el("span", t("ui.664939a1fa"), "message-time");
    const options = {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
    };
    const stamp = el("time", date.toDateString() === new Date().toDateString()
        ? date.toLocaleTimeString("zh-CN", options)
        : date.toLocaleString("zh-CN", {
            ...options,
            year: "numeric",
            month: "2-digit",
            day: "2-digit",
        }), "message-time");
    stamp.dateTime = date.toISOString();
    stamp.title = date.toLocaleString("zh-CN", {
        ...options,
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        timeZoneName: "short",
    });
    stamp.setAttribute("aria-label", stamp.title);
    return stamp;
}
export function svg(tag, attrs, text) {
    const n = document.createElementNS("http://www.w3.org/2000/svg", tag);
    Object.entries(attrs || {}).forEach(([k, v]) => n.setAttribute(k, v));
    if (text !== undefined)
        n.textContent = text;
    return n;
}
