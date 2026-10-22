import { useEffect, useState } from 'react'
import { api, defaultSettings, on, type EngineStatus, type Settings } from './api'

/** Settings kept in sync across windows; also applies theme, language and direction to the document. */
export function useSettings(view: string): Settings {
  const [s, setS] = useState<Settings>(defaultSettings)

  useEffect(() => {
    let alive = true
    api.getSettings().then((v) => alive && setS(v)).catch(() => {})
    const off = on<Settings>('settings-changed', setS)
    return () => {
      alive = false
      off()
    }
  }, [])

  const [sysDark, setSysDark] = useState(() => matchMedia('(prefers-color-scheme: dark)').matches)
  useEffect(() => {
    const mq = matchMedia('(prefers-color-scheme: dark)')
    const h = () => setSysDark(mq.matches)
    mq.addEventListener('change', h)
    return () => mq.removeEventListener('change', h)
  }, [])

  const effective = s.theme === 'system' ? (sysDark ? 'dark' : 'light') : s.theme
  useEffect(() => {
    const root = document.documentElement
    root.dataset.theme = effective
    root.lang = s.uiLanguage
    root.dir = s.uiLanguage === 'ar' ? 'rtl' : 'ltr'
    document.body.className = `view-${view}`
  }, [effective, s.uiLanguage, view])

  return s
}

export function useEngine(): EngineStatus | null {
  const [st, setSt] = useState<EngineStatus | null>(null)
  useEffect(() => {
    let alive = true
    const load = () => api.getEngineStatus().then((v) => alive && setSt(v)).catch(() => {})
    load()
    const off = on('engine-status', load)
    return () => {
      alive = false
      off()
    }
  }, [])
  return st
}
