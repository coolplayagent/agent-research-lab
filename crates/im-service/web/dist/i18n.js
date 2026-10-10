import { createI18n } from "/assets/shared/i18n.js";
import { messages as shared } from "/assets/shared/locales/zh-CN.js";
import { messages } from "./locales/zh-CN.js";
const catalogs = { "zh-CN": { ...shared, ...messages } };
export const { t, label, describe, apply, locale } = createI18n(catalogs, document.documentElement.lang, "zh-CN");
