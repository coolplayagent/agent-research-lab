/** Locale lookup is confined to presentation. Parameters are text, never HTML. */
export type Messages = Readonly<Record<string, string>>;
export type Parameters = Readonly<Record<string, unknown>>;
export declare function createI18n(catalogs: Readonly<Record<string, Messages>>, requested: string, fallback: string): {
    locale: string;
    t: (key: string, parameters?: Parameters) => string;
    has: (key: string) => boolean;
    describe: (value: unknown) => string;
    label: (domain: string, value: string) => string;
    apply: (root?: ParentNode) => void;
};
