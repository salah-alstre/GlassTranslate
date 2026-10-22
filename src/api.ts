import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export type UiLang = 'en' | 'ar'
export type Theme = 'dark' | 'light' | 'system'
/** Translation direction id; the target is always Arabic. */
export type Direction = 'en-ar' | 'he-ar'
export const DIRECTIONS: Direction[] = ['en-ar', 'he-ar']
export const sourceLangOf = (d: Direction | string): 'en' | 'he' => (d === 'he-ar' ? 'he' : 'en')
export const directionOf = (source: string): Direction => (source === 'he' ? 'he-ar' : 'en-ar')

export interface Settings {
  uiLanguage: UiLang
  theme: Theme
  shortcutCapture: string
  shortcutClipboard: string
  shortcutMain: string
  popupPinned: boolean
  popupFontSize: 's' | 'm' | 'l'
  popupOpacity: number
  popupRememberPosition: boolean
  popupPos: [number, number] | null
  historyEnabled: boolean
  direction: Direction
}

export interface AppInfo {
  version: string
  shortcutErrors: string[]
}

export interface ModelInfo {
  id: Direction
  label: string
  upstream: string
  license: string
  state: 'not_loaded' | 'loading' | 'ready' | 'failed'
  error: string | null
  files: 'ok' | 'missing'
  sizeBytes: number
  modelDir: string | null
  modelDirDisplay: string | null
}

export interface OcrInfo {
  direction: Direction
  available: boolean
  engine: 'windows' | 'tesseract'
  language: string | null
  detail: string | null
}

export interface EngineStatus {
  models: ModelInfo[]
  ocr: OcrInfo[]
}

export interface ModelCheck {
  state: 'ok' | 'missing' | 'corrupt'
  detail: string
  sizeBytes: number
}

export interface PopupState {
  status: 'idle' | 'loading' | 'done' | 'error'
  source: 'ocr' | 'clipboard' | 'text'
  original: string
  translation: string
  errorKind: string | null
  errorDetail: string | null
  direction: Direction
  sourceLang: 'en' | 'he'
  targetLang: 'ar'
  truncated: boolean
  gen: number
  session: number
  /** History entry id; 0 when history is disabled or nothing was recorded. */
  entryId: number
  favorite: boolean
}

export interface HistoryEntry {
  id: number
  original: string
  translation: string
  source: 'screen' | 'clipboard' | 'manual'
  ts: number
  favorite: boolean
  sourceLang: 'en' | 'he'
  targetLang: 'ar'
  method: string
}

export interface Selection {
  x: number
  y: number
  w: number
  h: number
  vw: number
  vh: number
}

export const defaultSettings: Settings = {
  uiLanguage: 'en',
  theme: 'dark',
  shortcutCapture: 'Ctrl+Alt+G',
  shortcutClipboard: 'Ctrl+Alt+V',
  shortcutMain: '',
  popupPinned: true,
  popupFontSize: 'm',
  popupOpacity: 100,
  popupRememberPosition: false,
  popupPos: null,
  historyEnabled: false,
  direction: 'en-ar',
}

export const api = {
  getSettings: () => invoke<Settings>('get_settings'),
  setSettings: (settings: Settings) => invoke<Settings>('set_settings', { settings }),
  getAppInfo: () => invoke<AppInfo>('get_app_info'),
  getEngineStatus: () => invoke<EngineStatus>('get_engine_status'),
  reloadEngine: (direction: Direction) => invoke<void>('reload_engine', { direction }),
  revealModelDir: (direction: Direction) => invoke<void>('reveal_model_dir', { direction }),
  verifyModel: (direction: Direction) => invoke<ModelCheck>('verify_model', { direction }),
  setDirection: (direction: Direction) => invoke<Settings>('set_direction', { direction }),
  startCapture: () => invoke<void>('start_capture'),
  clipboardTranslate: () => invoke<void>('clipboard_translate_cmd'),
  readClipboardText: () => invoke<string>('read_clipboard_text'),
  translateText: (text: string, direction?: Direction) =>
    invoke<{ translation: string; truncated: boolean }>('translate_text', { text, direction }),
  selectorReady: (gen: number) => invoke<void>('selector_ready', { gen }),
  selectionDone: (sel: Selection) => invoke<void>('selection_done', { sel }),
  selectionCancel: () => invoke<void>('selection_cancel'),
  getPopupState: () => invoke<PopupState>('get_popup_state'),
  resizePopup: (height: number) => invoke<void>('resize_popup', { height }),
  setPopupWide: (wide: boolean) => invoke<void>('set_popup_wide', { wide }),
  hidePopup: () => invoke<void>('hide_popup'),
  setPinned: (pinned: boolean) => invoke<void>('set_pinned', { pinned }),
  copyText: (text: string) => invoke<void>('copy_text', { text }),
  retranslate: (text: string) => invoke<void>('retranslate', { text }),
  historyList: () => invoke<HistoryEntry[]>('history_list'),
  historyDelete: (id: number) => invoke<void>('history_delete', { id }),
  historyClear: () => invoke<void>('history_clear'),
  historySetFavorite: (id: number, favorite: boolean) => invoke<void>('history_set_favorite', { id, favorite }),
  historyReopen: (id: number) => invoke<void>('history_reopen', { id }),
}

/** Subscribe to a backend event; returns a synchronous cleanup function. */
export function on<T = unknown>(event: string, cb: (payload: T) => void): () => void {
  let un: UnlistenFn | null = null
  let dead = false
  listen<T>(event, (e) => cb(e.payload)).then((f) => {
    if (dead) f()
    else un = f
  })
  return () => {
    dead = true
    un?.()
  }
}

/** Tauri rejects with a plain string for our commands; normalize anything else. */
export function errString(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
}
