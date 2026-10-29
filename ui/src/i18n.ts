// Traduction de l'interface. La langue est celle de Windows (donnée par le cœur Rust avant le
// premier affichage, voir src-tauri/src/i18n.rs), sinon celle du navigateur ; l'anglais sinon.
//
// Les textes sont dans ui/src/locales/ : en.ts est la référence, et TypeScript refuse de
// construire l'appli s'il manque un texte dans une autre langue.

import { en, type Key } from "./locales/en";
import { fr } from "./locales/fr";
import { es } from "./locales/es";
import { it } from "./locales/it";
import { pt } from "./locales/pt";
import { zh } from "./locales/zh";
import { ja } from "./locales/ja";

export type { Key };
export type Messages = Record<Key, string>;

const MESSAGES = { en, fr, es, it, pt, zh, ja } satisfies Record<string, Messages>;
export type Lang = keyof typeof MESSAGES;

/** La langue gérée la plus proche d'une étiquette comme « fr-FR », « zh-Hans-CN » ou « pt-BR ». */
export function pickLang(tags: readonly (string | undefined | null)[]): Lang {
  for (const tag of tags) {
    const base = tag?.toLowerCase().split(/[-_]/)[0];
    if (base && base in MESSAGES) return base as Lang;
  }
  return "en";
}

export const lang: Lang = pickLang([window.__DOCUMENTS_LANGUE__, ...(navigator.languages ?? []), navigator.language]);

/** L'étiquette donnée à Intl (dates, nombres) : celle de la variante traduite. */
export const locale = ({ en: "en-US", fr: "fr-FR", es: "es-ES", it: "it-IT", pt: "pt-PT", zh: "zh-CN", ja: "ja-JP" } as const)[lang];

const messages: Messages = MESSAGES[lang];
const plurals = new Intl.PluralRules(locale);

/** Un nombre écrit à la façon de la langue (« 1 234,5 » en français). */
export const num = (n: number, options?: Intl.NumberFormatOptions) => n.toLocaleString(locale, options);

function fill(text: string, params?: Record<string, string | number>): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (all, name: string) =>
    name in params ? (typeof params[name] === "number" ? num(params[name] as number) : String(params[name])) : all,
  );
}

/** Le texte traduit, avec ses {valeurs} remplacées. */
export function t(key: Key, params?: Record<string, string | number>): string {
  return fill(messages[key] ?? en[key], params);
}

/**
 * Un texte qui dépend d'un nombre : « singulier|pluriel » dans les fichiers de langue
 * (en chinois et en japonais, « un|plusieurs » quand la phrase change). {n} est remplacé par le nombre.
 */
export function tn(key: Key, n: number, params?: Record<string, string | number>): string {
  const forms = (messages[key] ?? en[key]).split("|");
  const form = forms.length > 1 && (n === 1 || plurals.select(n) === "one") ? forms[0] : forms[forms.length - 1];
  return fill(form, { n, ...params });
}

/**
 * Traduit la page : data-i18n (texte), data-i18n-title (bulle d'aide) et
 * data-i18n-placeholder (texte grisé d'un champ).
 */
export function translatePage(root: ParentNode = document) {
  document.documentElement.lang = locale;
  for (const el of root.querySelectorAll<HTMLElement>("[data-i18n]")) el.textContent = t(el.dataset.i18n as Key);
  for (const el of root.querySelectorAll<HTMLElement>("[data-i18n-title]")) el.title = t(el.dataset.i18nTitle as Key);
  for (const el of root.querySelectorAll<HTMLInputElement>("[data-i18n-placeholder]")) el.placeholder = t(el.dataset.i18nPlaceholder as Key);
}
