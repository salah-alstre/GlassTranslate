import { createContext, useCallback, useContext, useMemo, type ReactNode } from 'react'
import en, { type Dict, type Key } from './en'
import ar from './ar'

export type Lang = 'en' | 'ar'
export type { Key }

const dicts: Record<Lang, Dict> = { en, ar }

type Ctx = { lang: Lang; dir: 'ltr' | 'rtl'; t: (k: Key, vars?: Record<string, string>) => string }

const I18nContext = createContext<Ctx>({ lang: 'en', dir: 'ltr', t: (k) => en[k] })

export function I18nProvider({ lang, children }: { lang: Lang; children: ReactNode }) {
  const t = useCallback(
    (k: Key, vars?: Record<string, string>) => {
      let s = dicts[lang][k] ?? en[k] ?? k
      if (vars) for (const [name, v] of Object.entries(vars)) s = s.replaceAll(`{${name}}`, v)
      return s
    },
    [lang],
  )
  const value = useMemo<Ctx>(() => ({ lang, dir: lang === 'ar' ? 'rtl' : 'ltr', t }), [lang, t])
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>
}

export const useI18n = () => useContext(I18nContext)

/** Maps a backend error kind to a localized message. */
export function errorMessage(t: Ctx['t'], kind: string, direction?: string): string {
  const key = (kind === 'ocr_unavailable' && direction === 'he-ar' ? 'error.ocr_unavailable_he' : `error.${kind}`) as Key
  return key in en ? t(key) : t('error.generic')
}
