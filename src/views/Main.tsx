import { useEffect, useRef, useState, type ReactNode } from 'react'
import {
  AlertTriangle,
  Check,
  ClipboardPaste,
  Copy,
  History as HistoryIcon,
  Keyboard,
  Languages,
  ScanText,
  Settings as Cog,
  Star,
  X,
} from 'lucide-react'
import { api, DIRECTIONS, errString, on, type AppInfo, type Direction, type Settings } from '../api'
import { useEngine } from '../hooks'
import { errorMessage, useI18n, type Key } from '../i18n'
import HistoryView from './History'
import SettingsView from './Settings'
import { useToast } from './Toast'
import { Button, Card, Logo, Shortcut } from './ui'

type Page = 'home' | 'history' | 'favorites' | 'settings'
const MAX_CHARS = 4000

const tabs: { id: Page; label: Key; icon: typeof Languages }[] = [
  { id: 'home', label: 'nav.translate', icon: Languages },
  { id: 'history', label: 'nav.history', icon: HistoryIcon },
  { id: 'favorites', label: 'nav.favorites', icon: Star },
  { id: 'settings', label: 'nav.settings', icon: Cog },
]

export default function Main({ settings }: { settings: Settings }) {
  const { t } = useI18n()
  const [page, setPage] = useState<Page>('home')
  const [info, setInfo] = useState<AppInfo | null>(null)
  const engine = useEngine()

  useEffect(() => {
    api.getAppInfo().then(setInfo).catch(() => {})
    return on<string>('navigate', (p) => setPage(p === 'settings' ? 'settings' : 'home'))
  }, [])

  // Status follows the selected direction. A model that has not been touched yet ("not_loaded")
  // with intact files loads on first use, so it counts as ready.
  const cur = engine?.models.find((m) => m.id === settings.direction)
  const curOcr = engine?.ocr.find((o) => o.direction === settings.direction)
  const filesOk = cur?.files === 'ok'
  const failed = cur?.state === 'failed' || (!!cur && !filesOk)
  const loading = !cur || cur.state === 'loading'
  const ready = !failed && !loading && filesOk
  const ocrMissing = curOcr ? !curOcr.available : false
  const dot = failed ? 'bg-danger' : ready ? 'bg-accent2' : 'bg-warn'
  const label = failed ? t('status.failed') : ready ? t('status.ready') : t('status.loading')

  // Startup shortcut errors arrive as codes like "shortcut_conflict:main" or "shortcut_same".
  const slotName = (slot?: string): string =>
    slot === 'capture'
      ? t('settings.shortcutCapture')
      : slot === 'clipboard'
        ? t('settings.shortcutClipboard')
        : slot === 'main'
          ? t('settings.shortcutMain')
          : t('settings.shortcuts')
  const startupErrors = (info?.shortcutErrors ?? []).map((e) => {
    const [kind, slot] = e.split(':')
    if (kind === 'shortcut_same') return t('error.shortcut_same')
    return t('error.shortcut_startup', { what: slotName(slot) })
  })

  return (
    <div className="mx-auto flex min-h-screen max-w-[520px] flex-col gap-3.5 px-5 pb-5 pt-5">
      <header className="flex items-center gap-3">
        <Logo size={36} />
        <div className="min-w-0 flex-1 leading-tight">
          <h1 className="truncate text-[17px] font-semibold tracking-tight">{t('app.name')}</h1>
          <div className="mt-0.5 text-[12px] text-muted">{t('main.subtitle')}</div>
        </div>
        <span
          className="inline-flex shrink-0 items-center gap-1.5 rounded-full border border-line bg-surface px-2.5 py-1 text-[11.5px] text-muted"
          role="status"
        >
          <span className={`inline-block h-2 w-2 rounded-full ${dot} ${loading ? 'animate-pulse' : ''}`} />
          {label}
        </span>
      </header>

      <nav aria-label="Pages" className="grid grid-cols-4 gap-1 rounded-xl border border-line bg-surface p-1">
        {tabs.map(({ id, label: l, icon: Icon }) => (
          <button
            key={id}
            type="button"
            onClick={() => setPage(id)}
            aria-current={page === id ? 'page' : undefined}
            className={`flex min-w-0 items-center justify-center gap-1.5 rounded-lg px-1.5 py-1.5 text-[12.5px] font-medium transition-colors ${
              page === id ? 'bg-raised text-accent ring-1 ring-line' : 'text-muted hover:text-fg'
            }`}
          >
            <Icon size={15} className="shrink-0" />
            <span className="truncate">{t(l)}</span>
          </button>
        ))}
      </nav>

      {startupErrors.map((m) => (
        <Notice key={m}>{m}</Notice>
      ))}
      {ocrMissing && <Notice>{settings.direction === 'he-ar' ? t('status.ocrMissingHe') : t('status.ocrMissing')}</Notice>}
      {failed && <Notice>{cur?.error || t('error.engine_not_ready')}</Notice>}

      {/* The translate page stays mounted so typed text and results survive switching tabs. */}
      <div className={page === 'home' ? 'flex flex-1 flex-col gap-3.5' : 'hidden'}>
        <Home settings={settings} ready={ready} />
      </div>
      {page === 'history' && <HistoryView settings={settings} mode="all" />}
      {page === 'favorites' && <HistoryView settings={settings} mode="favorites" />}
      {page === 'settings' && <SettingsView settings={settings} info={info} engine={engine} />}

      <footer className="mt-auto flex items-center justify-between pt-1 text-[11.5px] text-faint">
        <span>{t(`dir.label.${settings.direction}` as Key)}</span>
        {info && <span dir="ltr">{t('main.version', { v: info.version })}</span>}
      </footer>
    </div>
  )
}

function Notice({ children }: { children: ReactNode }) {
  return (
    <div role="alert" className="flex items-start gap-2.5 rounded-xl border border-warn/40 bg-warn/10 px-3.5 py-2.5 text-[12.5px] leading-relaxed">
      <AlertTriangle size={16} className="mt-0.5 shrink-0 text-warn" />
      <span className="min-w-0">{children}</span>
    </div>
  )
}

function Home({ settings, ready }: { settings: Settings; ready: boolean }) {
  const { t } = useI18n()
  const toast = useToast()
  const inputRef = useRef<HTMLTextAreaElement>(null)

  const run = (p: Promise<unknown>) =>
    p.catch((e) => toast(errorMessage(t, errString(e)), 'error'))

  const pick = (d: Direction) => {
    if (d !== settings.direction) run(api.setDirection(d))
  }

  return (
    <>
      <div
        role="radiogroup"
        aria-label={t('main.directionLabel')}
        className="grid grid-cols-2 gap-1 rounded-xl border border-line bg-surface p-1"
      >
        {DIRECTIONS.map((d) => {
          const on = settings.direction === d
          return (
            <button
              key={d}
              type="button"
              role="radio"
              aria-checked={on}
              onClick={() => pick(d)}
              className={`min-w-0 truncate rounded-lg px-2 py-1.5 text-[13px] font-medium transition-colors ${
                on ? 'bg-raised text-accent ring-1 ring-line' : 'text-muted hover:text-fg'
              }`}
            >
              {t(`dir.label.${d}` as Key)}
            </button>
          )
        })}
      </div>

      <Card title={t('main.quickActions')}>
        <div className="grid grid-cols-3 gap-2.5">
          <Action
            primary
            icon={<ScanText size={22} />}
            title={t('main.select')}
            hint={t('main.selectHint')}
            onClick={() => run(api.startCapture())}
          />
          <Action
            icon={<ClipboardPaste size={22} />}
            title={t('main.clipboard')}
            hint={t('main.clipboardHint')}
            onClick={() => run(api.clipboardTranslate())}
          />
          <Action
            icon={<Keyboard size={22} />}
            title={t('main.typed')}
            hint={t('main.typedHint')}
            onClick={() => inputRef.current?.focus()}
          />
        </div>
      </Card>

      <QuickText ready={ready} direction={settings.direction} inputRef={inputRef} />

      <Card title={t('main.shortcuts')}>
        <ul className="space-y-2.5 text-[13px]">
          <ShortcutRow label={t('settings.shortcutCapture')} value={settings.shortcutCapture} />
          <ShortcutRow label={t('settings.shortcutClipboard')} value={settings.shortcutClipboard} />
          {settings.shortcutMain && <ShortcutRow label={t('settings.shortcutMain')} value={settings.shortcutMain} />}
        </ul>
      </Card>
    </>
  )
}

function ShortcutRow({ label, value }: { label: string; value: string }) {
  return (
    <li className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1.5">
      <span className="text-muted">{label}</span>
      <Shortcut value={value} />
    </li>
  )
}

function Action({
  icon,
  title,
  hint,
  onClick,
  primary,
}: {
  icon: ReactNode
  title: string
  hint: string
  onClick: () => void
  primary?: boolean
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={hint}
      className={`flex min-w-0 flex-col items-start gap-2 rounded-xl border p-3 text-start transition active:scale-[0.98] ${
        primary
          ? 'btn-primary border-transparent'
          : 'border-line bg-raised hover:border-accent/60'
      }`}
    >
      <span className={primary ? '' : 'text-accent'}>{icon}</span>
      <span className="block w-full min-w-0">
        <span className="block break-words text-[13px] font-semibold leading-tight">{title}</span>
        <span className={`mt-0.5 block break-words text-[11px] leading-snug ${primary ? 'opacity-75' : 'text-muted'}`}>{hint}</span>
      </span>
    </button>
  )
}

function QuickText({
  ready,
  direction,
  inputRef,
}: {
  ready: boolean
  direction: Direction
  inputRef: React.RefObject<HTMLTextAreaElement | null>
}) {
  const { t } = useI18n()
  const toast = useToast()
  const [text, setText] = useState('')
  const [out, setOut] = useState<{ translation: string; truncated: boolean } | null>(null)
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const he = direction === 'he-ar'

  // A result belongs to the direction it was made in; drop it when the direction changes.
  useEffect(() => {
    setOut(null)
    setErr(null)
  }, [direction])

  useEffect(() => {
    if (!copied) return
    const id = setTimeout(() => setCopied(false), 1600)
    return () => clearTimeout(id)
  }, [copied])

  const run = async () => {
    if (busy) return
    if (!text.trim()) {
      setErr(t(he ? 'main.emptyInputHe' : 'main.emptyInput'))
      return
    }
    setBusy(true)
    setErr(null)
    setOut(null)
    try {
      setOut(await api.translateText(text, direction))
    } catch (e) {
      const msg = errorMessage(t, errString(e))
      setErr(msg)
      toast(t('toast.translateFailed'), 'error')
    } finally {
      setBusy(false)
    }
  }

  const paste = async () => {
    setErr(null)
    try {
      setText((await api.readClipboardText()).slice(0, MAX_CHARS))
      inputRef.current?.focus()
    } catch (e) {
      const k = errString(e)
      setErr(k === 'clipboard_empty' ? t('main.pasteEmpty') : errorMessage(t, k))
    }
  }

  const clear = () => {
    setText('')
    setOut(null)
    setErr(null)
    inputRef.current?.focus()
  }

  const copy = async () => {
    if (!out) return
    try {
      await api.copyText(out.translation)
      setCopied(true)
      toast(t('toast.copied'))
    } catch (e) {
      toast(errorMessage(t, errString(e)), 'error')
    }
  }

  const near = text.length > MAX_CHARS * 0.9

  return (
    <Card title={t('main.quickTitle')}>
      <textarea
        ref={inputRef}
        value={text}
        maxLength={MAX_CHARS}
        onChange={(e) => {
          setText(e.target.value)
          if (err) setErr(null)
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
            e.preventDefault()
            run()
          }
        }}
        dir={he ? 'rtl' : 'ltr'}
        lang={he ? 'he' : 'en'}
        rows={4}
        aria-label={t('main.quickTitle')}
        placeholder={t(he ? 'main.placeholderHe' : 'main.placeholder')}
        className="w-full resize-none rounded-xl border border-line bg-bg px-3 py-2.5 text-[14px] leading-relaxed text-fg outline-none transition placeholder:text-faint focus:border-accent/70"
      />
      <div className="mt-1.5 flex items-center justify-between gap-2 text-[11px] text-faint">
        <span>{t('main.hintCtrlEnter')}</span>
        <span dir="ltr" className={near ? 'text-warn' : ''}>
          {t('main.chars', { n: String(text.length), max: String(MAX_CHARS) })}
        </span>
      </div>
      <div className="mt-2.5 flex flex-wrap items-center gap-2">
        <Button variant="primary" onClick={run} disabled={!text.trim() || busy || !ready}>
          {busy ? t('main.translating') : t('main.translate')}
        </Button>
        <Button onClick={paste}>
          <ClipboardPaste size={14} />
          {t('main.paste')}
        </Button>
        {(text || out) && (
          <Button variant="ghost" onClick={clear} className="ms-auto">
            <X size={14} />
            {t('main.clear')}
          </Button>
        )}
      </div>

      {err && (
        <div role="alert" className="anim-fade mt-3 text-[12.5px] leading-relaxed text-warn">
          {err}
        </div>
      )}

      {busy && (
        <div className="mt-3 space-y-2" aria-busy="true">
          <div className="skeleton h-3 w-11/12 rounded" />
          <div className="skeleton h-3 w-8/12 rounded" />
        </div>
      )}

      {out ? (
        <div className="anim-fade mt-3 border-t border-soft pt-3">
          <div
            dir="rtl"
            lang="ar"
            className="selectable whitespace-pre-wrap break-words font-arabic text-[16.5px] leading-[1.85]"
          >
            {out.translation}
          </div>
          {out.truncated && <div className="mt-1.5 text-[11px] text-warn">{t('main.truncated')}</div>}
          <Button onClick={copy} className="mt-2.5">
            {copied ? <Check size={14} className="text-accent2" /> : <Copy size={14} />}
            {copied ? t('main.copied') : t('main.copy')}
          </Button>
        </div>
      ) : (
        !busy &&
        !err && (
          <p className="mt-3 border-t border-soft pt-3 text-center text-[12px] text-faint">{t('main.resultEmpty')}</p>
        )
      )}
    </Card>
  )
}
