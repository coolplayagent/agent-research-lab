export function createI18n(catalogs, requested, fallback) {
    if (!catalogs[fallback])
        throw new Error(`Missing fallback locale: ${fallback}`);
    const locale = catalogs[requested] ? requested : fallback;
    const messages = catalogs[locale];
    function has(key) {
        return (Object.hasOwn(messages, key) || Object.hasOwn(catalogs[fallback], key));
    }
    function t(key, parameters = {}) {
        if (!has(key))
            throw new Error(`Missing translation: ${key}`);
        return (messages[key] ?? catalogs[fallback][key]).replace(/\{([a-zA-Z][a-zA-Z0-9_]*)\}/g, (_, name) => {
            if (!Object.hasOwn(parameters, name))
                throw new Error(`Missing parameter ${name} for ${key}`);
            return String(parameters[name] ?? "");
        });
    }
    function describe(value) {
        if (value &&
            typeof value === "object" &&
            "code" in value &&
            typeof value.code === "string") {
            const notice = value;
            return has(`api.${notice.code}`)
                ? t(`api.${notice.code}`, notice.parameters)
                : notice.code;
        }
        return value == null ? "" : String(value);
    }
    function label(domain, value) {
        const key = `${domain}.${value}`;
        return has(key) ? t(key) : value;
    }
    function apply(root = document) {
        root.querySelectorAll("[data-i18n]").forEach((node) => {
            node.textContent = t(node.dataset.i18n);
        });
        for (const attribute of ["placeholder", "title", "aria-label"]) {
            root
                .querySelectorAll(`[data-i18n-${attribute}]`)
                .forEach((node) => {
                node.setAttribute(attribute, t(node.getAttribute(`data-i18n-${attribute}`)));
            });
        }
    }
    return { locale, t, has, describe, label, apply };
}
