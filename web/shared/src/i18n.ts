/** Locale lookup is confined to presentation. Parameters are text, never HTML. */
export type Messages = Readonly<Record<string, string>>;
export type Parameters = Readonly<Record<string, unknown>>;
export function createI18n(
  catalogs: Readonly<Record<string, Messages>>,
  requested: string,
  fallback: string,
) {
  if (!catalogs[fallback])
    throw new Error(`Missing fallback locale: ${fallback}`);
  const locale = catalogs[requested] ? requested : fallback;
  const messages = catalogs[locale];
  function has(key: string): boolean {
    return (
      Object.hasOwn(messages, key) || Object.hasOwn(catalogs[fallback], key)
    );
  }
  function t(key: string, parameters: Parameters = {}): string {
    if (!has(key)) throw new Error(`Missing translation: ${key}`);
    return (messages[key] ?? catalogs[fallback][key]).replace(
      /\{([a-zA-Z][a-zA-Z0-9_]*)\}/g,
      (_, name: string) => {
        if (!Object.hasOwn(parameters, name))
          throw new Error(`Missing parameter ${name} for ${key}`);
        return String(parameters[name] ?? "");
      },
    );
  }
  function describe(value: unknown): string {
    if (
      value &&
      typeof value === "object" &&
      "code" in value &&
      typeof value.code === "string"
    ) {
      const notice = value as { code: string; parameters?: Parameters };
      return has(`api.${notice.code}`)
        ? t(`api.${notice.code}`, notice.parameters)
        : notice.code;
    }
    return value == null ? "" : String(value);
  }
  function label(domain: string, value: string): string {
    const key = `${domain}.${value}`;
    return has(key) ? t(key) : value;
  }
  function apply(root: ParentNode = document): void {
    root.querySelectorAll<HTMLElement>("[data-i18n]").forEach((node) => {
      node.textContent = t(node.dataset.i18n!);
    });
    for (const attribute of ["placeholder", "title", "aria-label"]) {
      root
        .querySelectorAll<HTMLElement>(`[data-i18n-${attribute}]`)
        .forEach((node) => {
          node.setAttribute(
            attribute,
            t(node.getAttribute(`data-i18n-${attribute}`)!),
          );
        });
    }
  }
  return { locale, t, has, describe, label, apply };
}
