import { createEffect, onCleanup } from "solid-js";

const FONT_MAP = {
  en: { family: "Inter", weights: "400;500;700" },
  fr: { family: "Inter", weights: "400;500;700" },
  de: { family: "Inter", weights: "400;500;700" },
  ru: { family: "Noto+Sans", weights: "400;500;700" },
  "zh-hans": { family: "Noto+Sans+SC", weights: "400;500;700" },
  "zh-hant": { family: "Noto+Sans+TC", weights: "400;500;700" },
  ja: { family: "Noto+Sans+JP", weights: "400;500;700" },
  ko: { family: "Noto+Sans+KR", weights: "400;500;700" },
  hi: { family: "Noto+Sans+Devanagari", weights: "400;500;700" },
  th: { family: "Noto+Sans+Thai", weights: "400;500;700" },
  bn: { family: "Noto+Sans+Bengali", weights: "400;500;700" },
  ar: { family: "Noto+Sans+Arabic", weights: "400;500;700" },
  he: { family: "Noto+Sans+Hebrew", weights: "400;500;700" },
  fa: { family: "Noto+Sans+Arabic", weights: "400;500;700" },
} as const satisfies Record<string, { family: string; weights: string }>;

export type Lang = keyof typeof FONT_MAP;

const loaded = new Set<string>();

function filterValidLangs(langs: string[]): Lang[] {
  return langs.filter((lang): lang is Lang => {
    if (lang in FONT_MAP) return true;
    if (!import.meta.env.PROD) {
      console.warn(
        `[useFontLoader] unsupported lang: "${lang}". ` +
          `Supported values: ${Object.keys(FONT_MAP).join(", ")}`,
      );
    }
    return false;
  });
}

export interface UseFontLoaderOptions {
  /**
   *
   */
  langs?: Lang | Lang[];

  /**
   *
   */
  targetRef?: HTMLElement;

  /**
   * @default
   */
  apply?: boolean;
}

function loadGoogleFonts(langs: Lang[]): void {
  const toLoad = langs.filter((lang) => !loaded.has(FONT_MAP[lang].family));
  if (toLoad.length === 0) return;

  const families = [
    ...new Set(
      toLoad.map((lang) => {
        const { family, weights } = FONT_MAP[lang];
        return `family=${family}:wght@${weights}`;
      }),
    ),
  ];

  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = `https://fonts.googleapis.com/css2?${families.join("&")}&display=swap`;

  document.head.appendChild(link);
  for (const lang of toLoad) {
    loaded.add(FONT_MAP[lang].family);
  }
}

// ─── Hook ─────────────────────────────────────────────────────
export function useFontLoader(options?: UseFontLoaderOptions) {
  const { langs = "en", targetRef = document.documentElement, apply = true } = options ?? {};

  const langList = Array.isArray(langs) ? langs : [langs];
  const validLangs = filterValidLangs(langList);

  createEffect(() => {
    loadGoogleFonts(validLangs);
  });

  createEffect(() => {
    if (!apply || validLangs.length === 0) return;

    const el = targetRef;
    const lang = validLangs[0];
    const prev = el.getAttribute("data-lang");

    el.setAttribute("data-lang", lang);

    onCleanup(() => {
      if (prev === null) {
        el.removeAttribute("data-lang");
      } else {
        el.setAttribute("data-lang", prev);
      }
    });
  });
}
