import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  AlertTriangle,
  Check,
  ChevronDown,
  ChevronUp,
  Copy,
  Maximize2,
  Minimize2,
  Pin,
  RefreshCw,
  Star,
  X,
} from 'lucide-react'
import { api, on, type PopupState, type Settings } from '../api'
import { errorMessage, useI18n, type Key } from '../i18n'
import { Logo } from './ui'

const idle: PopupState = {
  status: 'idle',
  source: 'ocr',
  original: '',
  translation: '',
  errorKind: null,
  errorDetail: null,
  truncated: false,
  gen: 0,
  session: 0,
  entryId: 0,
  favorite: false,
  direction: 'en-ar',
  sourceLang: 'en',
  targetLang: 'ar',
}

export default function Popup({ settings }: { settings: Settings }) {
  const { t, dir } = useI18n()
  const [st, setSt] = useState<PopupState>(idle)
  const [showOrig, setShowOrig] = useState(false)
  const [wide, setWide] = useState(false)
  const [copied, setCopied] = useState<'tr' | 'orig' | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const pinned = settings.popupPinned
  const wrap = useRef<HTMLDivElement>(null)
  const lastH = useRef(0)
  const session = useRef(-1)

  const apply = useCallback((s: PopupState) => {
    setSt(s)
    if (s.status === 'idle' || s.session !== session.current) {
      session.current = s.status === 'idle' ? session.current : s.session
      setWide(false)
      setShowOrig(false)
      setCopied(null)
      setNote(null)
    }
  }, [])

  useEffect(() => {
    api.getPopupState().then(apply).catch(() => {})
    return on<PopupState>('popup-update', apply)
  }, [apply])

  // Esc closes the popup — only while the popup itself has the keyboard focus.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') api.hidePopup().catch(() => {})
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  // The window is sized by its content: report the wrapper height (which includes the 12px margin).
  useLayoutEffect(() => {
    const el = wrap.current
    if (!el) return
    const report = () => {
      const h = Math.ceil(el.getBoundingClientRect().height)
      if (h > 0 && Math.abs(h - lastH.current) >= 1) {
        lastH.current = h
        api.resizePopup(h).catch(() => {})
      }
    }
    report()
    const ro = new ResizeObserver(report)
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  useEffect(() => {
    if (!copied) return
    const id = setTimeout(() => setCopied(null), 1600)
    return () => clearTimeout(id)
  }, [copied])

  useEffect(() => {
    if (!note) return
    const id = setTimeout(() => setNote(null), 4200)
    return () => clearTimeout(id)
  }, [note])

  const drag = (e: React.MouseEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('button')) return
    e.preventDefault() // no text selection while dragging
    getCurrentWindow().startDragging().catch(() => {})
  }

  const copy = async (which: 'tr' | 'orig') => {
    const text = which === 'tr' ? st.translation : st.original
    if (!text) return
    try {
      await api.copyText(text)
      setCopied(which)
    } catch {
      /* the clipboard was unavailable; the button simply does not confirm */
    }
  }

  const toggleFavorite = async () => {
    if (!settings.historyEnabled || st.entryId <= 0) {
      setNote(t('toast.historyOff'))
      return
    }
    try {
      await api.historySetFavorite(st.entryId, !st.favorite)
    } catch (e) {
      setNote(errorMessage(t, typeof e === 'string' ? e : 'generic'))
    }
  }

  const retry = () => {
    if (st.original.trim()) api.retranslate(st.original).catch(() => {})
    else if (st.source === 'ocr') api.startCapture().catch(() => {})
    else if (st.source === 'clipboard') api.clipboardTranslate().catch(() => {})
  }

  const toggleWide = () => {
    const v = !wide
    setWide(v)
    api.setPopupWide(v).catch(() => {})
  }

  const loading = st.status === 'loading'
  const done = st.status === 'done'
  const error = st.status === 'error'
  const canAgain = !loading && st.original.trim().length > 0 && (done || error)
  const sizeCls = `popup-text-${settings.popupFontSize}`
  const cardStyle = { '--popup-alpha': `${settings.popupOpacity}%` } as CSSProperties

  return (
    <div ref={wrap} className="p-3">
      <div
        key={st.session}
        className="popup-card anim-pop overflow-hidden rounded-2xl text-fg"
        style={cardStyle}
      >
        <div
          onMouseDown={drag}
          className="flex cursor-grab select-none items-center gap-2 border-b border-soft bg-raised/60 px-3 py-2 active:cursor-grabbing"
        >
          <Logo size={20} />
          <div className="min-w-0 flex-1 leading-tight">
            <div className="truncate text-[12.5px] font-semibold">{t('app.name')}</div>
            <div dir="ltr" className="truncate text-start text-[11px] text-muted">
              {t(`dir.label.${st.direction}` as Key)}
            </div>
          </div>
          <IconBtn
            label={pinned ? t('popup.unpin') : t('popup.pin')}
            active={pinned}
            onClick={() => api.setPinned(!pinned).catch(() => {})}
          >
            <Pin size={14} className={pinned ? '' : '-rotate-45'} />
          </IconBtn>
          <IconBtn
            label={copied === 'tr' ? t('popup.copied') : t('popup.copy')}
            active={copied === 'tr'}
            disabled={!done}
            onClick={() => copy('tr')}
          >
            {copied === 'tr' ? <Check size={14} /> : <Copy size={14} />}
          </IconBtn>
          <IconBtn label={wide ? t('popup.collapse') : t('popup.expand')} onClick={toggleWide}>
            {wide ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
          </IconBtn>
          <IconBtn label={t('popup.close')} onClick={() => api.hidePopup().catch(() => {})}>
            <X size={15} />
          </IconBtn>
        </div>

        <div className="px-4 py-3">
          {loading && (
            <div className="anim-fade space-y-2.5 py-1" aria-live="polite">
              <div className="flex items-center gap-2 text-[12px] text-muted">
                <span className="spin inline-block h-3.5 w-3.5 rounded-full border-2 border-line border-t-accent" />
                {t('popup.translating')}
              </div>
              <div className="skeleton h-3 w-11/12 rounded" />
              <div className="skeleton h-3 w-9/12 rounded" />
              <div className="skeleton h-3 w-10/12 rounded" />
            </div>
          )}

          {done && (
            <div
              dir="rtl"
              lang="ar"
              className={`selectable anim-fade max-h-[300px] overflow-y-auto whitespace-pre-wrap break-words py-1 pe-1 text-start font-arabic ${sizeCls}`}
            >
              {st.translation}
            </div>
          )}

          {error && (
            <div className="anim-fade flex items-start gap-3 py-1" role="alert" dir={dir}>
              <AlertTriangle size={18} className="mt-0.5 shrink-0 text-warn" />
              <div className="min-w-0 text-[13px] leading-relaxed">
                <div>{errorMessage(t, st.errorKind ?? '', st.direction)}</div>
                {st.errorDetail && (
                  <div dir="ltr" className="selectable mt-1 break-words text-[11px] text-faint">
                    {st.errorDetail}
                  </div>
                )}
              </div>
            </div>
          )}

          {done && st.truncated && <div className="mt-2 text-[11px] text-warn">{t('popup.truncated')}</div>}

          {showOrig && st.original && (
            <div className="anim-fade mt-3 border-t border-soft pt-2.5">
              <div className="mb-1 flex items-center justify-between gap-2">
                <span className="text-[10.5px] font-semibold uppercase tracking-wider text-faint">
                  {t('popup.original')}
                </span>
                <button
                  type="button"
                  onClick={() => copy('orig')}
                  className="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-muted hover:bg-raised hover:text-fg"
                >
                  {copied === 'orig' ? <Check size={12} /> : <Copy size={12} />}
                  {copied === 'orig' ? t('popup.copied') : t('popup.copyOriginal')}
                </button>
              </div>
              <div
                dir={st.sourceLang === 'he' ? 'rtl' : 'auto'}
                lang={st.sourceLang}
                className="selectable max-h-[130px] text-start overflow-y-auto whitespace-pre-wrap pb-1 break-words text-[12.5px] leading-relaxed text-muted"
              >
                {st.original}
              </div>
            </div>
          )}

          {note && (
            <div role="status" className="anim-fade mt-2 rounded-lg bg-raised px-2.5 py-1.5 text-[11.5px] text-muted">
              {note}
            </div>
          )}
        </div>

        {(done || error) && (
          <div className="flex flex-wrap items-center gap-2 border-t border-soft px-3 py-2.5">
            {done && (
              <button
                type="button"
                onClick={() => copy('tr')}
                className="btn-primary inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[12.5px] font-semibold transition active:scale-[0.97]"
              >
                {copied === 'tr' ? <Check size={14} /> : <Copy size={14} />}
                {copied === 'tr' ? t('popup.copied') : t('popup.copy')}
              </button>
            )}
            {error && (
              <button
                type="button"
                onClick={retry}
                className="btn-primary inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[12.5px] font-semibold transition active:scale-[0.97]"
              >
                <RefreshCw size={13} />
                {t('popup.retry')}
              </button>
            )}
            {st.original && (
              <GhostBtn onClick={() => setShowOrig((v) => !v)}>
                {showOrig ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                {showOrig ? t('popup.hideOriginal') : t('popup.showOriginal')}
              </GhostBtn>
            )}
            {done && canAgain && (
              <GhostBtn onClick={() => api.retranslate(st.original).catch(() => {})}>
                <RefreshCw size={13} />
                {t('popup.again')}
              </GhostBtn>
            )}
            {done && (
              <GhostBtn onClick={toggleFavorite} active={st.favorite} pressed={st.favorite}>
                <Star size={13} className={st.favorite ? 'fill-current' : ''} />
                {st.favorite ? t('popup.saved') : t('popup.save')}
              </GhostBtn>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

function IconBtn({
  label,
  children,
  onClick,
  active,
  disabled,
}: {
  label: string
  children: ReactNode
  onClick: () => void
  active?: boolean
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={active}
      disabled={disabled}
      onClick={onClick}
      className={`grid h-7 w-7 place-items-center rounded-md transition hover:bg-line/70 disabled:opacity-40 disabled:hover:bg-transparent ${
        active ? 'text-accent' : 'text-muted hover:text-fg'
      }`}
    >
      {children}
    </button>
  )
}

function GhostBtn({
  children,
  onClick,
  active,
  pressed,
}: {
  children: ReactNode
  onClick: () => void
  active?: boolean
  pressed?: boolean
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={pressed}
      className={`inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-[12.5px] font-medium transition hover:text-fg active:scale-[0.97] ${
        active ? 'border-accent/60 text-accent' : 'border-line text-muted hover:border-accent/60'
      }`}
    >
      {children}
    </button>
  )
}
