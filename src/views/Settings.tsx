import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Copy, FolderOpen, Monitor, Moon, RefreshCw, ShieldCheck, Sun, Trash2 } from 'lucide-react'
import {
  api,
  DIRECTIONS,
  errString,
  type AppInfo,
  type Direction,
  type EngineStatus,
  type ModelCheck,
  type ModelInfo,
  type Settings,
  type Theme,
  type UiLang,
} from '../api'
import { useI18n, type Key } from '../i18n'
import { useToast } from './Toast'
import { Button, Card, Dialog, Logo, Row, Segmented, Shortcut, Toggle } from './ui'

type Slot = 'shortcutCapture' | 'shortcutClipboard' | 'shortcutMain'
const slotKey: Record<Slot, Key> = {
  shortcutCapture: 'settings.shortcutCapture',
  shortcutClipboard: 'settings.shortcutClipboard',
  shortcutMain: 'settings.shortcutMain',
}

/** Builds "Ctrl+Alt+G" from a keydown, or null while only modifiers are held / the key is unsupported. */
function comboFrom(e: KeyboardEvent): string | null {
  if (['Control', 'Alt', 'Shift', 'Meta', 'AltGraph'].includes(e.key)) return null
  const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean) as string[]
  let key: string | null = null
  const c = e.code
  if (/^Key[A-Z]$/.test(c)) key = c.slice(3)
  else if (/^Digit[0-9]$/.test(c)) key = c.slice(5)
  else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(c)) key = c
  else if (/^Numpad[0-9]$/.test(c)) key = c
  else {
    const map: Record<string, string> = {
      Space: 'Space', Enter: 'Enter', Tab: 'Tab', Backspace: 'Backspace', Delete: 'Delete', Insert: 'Insert',
      Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown',
      ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right',
      Minus: '-', Equal: '=', Comma: ',', Period: '.', Slash: '/', Semicolon: ';', Quote: "'", Backslash: '\\',
      BracketLeft: '[', BracketRight: ']', Backquote: '`',
    }
    key = map[c] ?? null
  }
  if (!key) return null
  return [...mods, key].join('+')
}

export default function SettingsView({
  settings,
  info,
  engine,
}: {
  settings: Settings
  info: AppInfo | null
  engine: EngineStatus | null
}) {
  const { t } = useI18n()
  const toast = useToast()
  const [err, setErr] = useState<string | null>(null)
  const [recording, setRecording] = useState<Slot | null>(null)
  const [reloading, setReloading] = useState<Direction | null>(null)
  const [verifying, setVerifying] = useState<Direction | null>(null)
  const [checks, setChecks] = useState<Partial<Record<Direction, ModelCheck>>>({})
  const [confirmClear, setConfirmClear] = useState(false)
  const [opacity, setOpacity] = useState(settings.popupOpacity)
  const opacityTimer = useRef<number | undefined>(undefined)
  const latest = useRef(settings)
  latest.current = settings

  useEffect(() => () => clearTimeout(opacityTimer.current), [])
  // Follow external changes (e.g. another window) when the slider is idle.
  useEffect(() => setOpacity(settings.popupOpacity), [settings.popupOpacity])

  const save = async (next: Settings): Promise<boolean> => {
    setErr(null)
    try {
      await api.setSettings(next)
      toast(t('toast.settingsSaved'))
      return true
    } catch (e) {
      const raw = errString(e)
      const [kind, slot] = raw.split(':')
      const which = slot && slot in slotKey ? t(slotKey[slot as Slot]) : null
      const msg =
        kind === 'shortcut_invalid'
          ? t('error.shortcut_invalid')
          : kind === 'shortcut_conflict'
            ? t('error.shortcut_conflict')
            : kind === 'shortcut_same'
              ? t('error.shortcut_same')
              : t('error.generic')
      setErr(which ? `${which}: ${msg}` : msg)
      return false
    }
  }

  // Capture the next key combination while a shortcut slot is being recorded.
  useEffect(() => {
    if (!recording) return
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault()
      e.stopPropagation()
      if (e.key === 'Escape') {
        setRecording(null)
        return
      }
      const combo = comboFrom(e)
      if (!combo) return
      const slot = recording
      setRecording(null)
      if (!/(^|\+)(Ctrl|Alt|Shift|Super)\+/.test(combo)) {
        setErr(`${t(slotKey[slot])}: ${t('error.shortcut_invalid')}`)
        return
      }
      save({ ...latest.current, [slot]: combo })
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recording])

  const onOpacity = (v: number) => {
    setOpacity(v)
    clearTimeout(opacityTimer.current)
    opacityTimer.current = window.setTimeout(() => {
      save({ ...latest.current, popupOpacity: v })
    }, 350)
  }

  const reload = async (d: Direction) => {
    setReloading(d)
    toast(t('toast.engineReloading'), 'info')
    try {
      await api.reloadEngine(d)
    } catch {
      /* the engine-status event reports the failure */
    } finally {
      setReloading(null)
    }
  }

  const verify = async (d: Direction) => {
    setVerifying(d)
    try {
      const c = await api.verifyModel(d)
      setChecks((p) => ({ ...p, [d]: c }))
    } catch (e) {
      toast(errString(e), 'error')
    } finally {
      setVerifying(null)
    }
  }

  const copyPath = async (m: ModelInfo) => {
    if (!m.modelDir) return
    try {
      await api.copyText(m.modelDir)
      toast(t('toast.pathCopied'))
    } catch (e) {
      toast(errString(e), 'error')
    }
  }

  const setDirection = async (d: Direction) => {
    if (d === settings.direction) return
    try {
      await api.setDirection(d)
      toast(t('toast.settingsSaved'))
    } catch (e) {
      toast(errString(e), 'error')
    }
  }

  const clearHistory = async () => {
    setConfirmClear(false)
    try {
      await api.historyClear()
      toast(t('toast.historyCleared'))
    } catch (e) {
      toast(errString(e), 'error')
    }
  }

  const mb = (b: number) => `${(b / 1048576).toFixed(0)} MB`

  return (
    <div className="anim-fade flex flex-col gap-3.5 pb-2">
      {err && (
        <div role="alert" className="rounded-xl border border-danger/40 bg-danger/10 px-3.5 py-2.5 text-[12.5px] leading-relaxed">
          {err}
        </div>
      )}

      <Card title={t('settings.sec.general')}>
        <Row title={t('settings.language')} help={t('settings.languageHelp')}>
          <Segmented<UiLang>
            label={t('settings.language')}
            value={settings.uiLanguage}
            onChange={(v) => v !== settings.uiLanguage && save({ ...settings, uiLanguage: v })}
            options={[
              { value: 'en', label: 'English' },
              { value: 'ar', label: 'العربية' },
            ]}
          />
        </Row>
        <p className="border-t border-soft pt-3 text-xs leading-relaxed text-faint">{t('settings.trayNote')}</p>
      </Card>

      <Card title={t('settings.sec.appearance')}>
        <Row title={t('settings.appearance')} help={t('settings.themeHelp')}>
          <Segmented<Theme>
            label={t('settings.appearance')}
            value={settings.theme}
            onChange={(v) => v !== settings.theme && save({ ...settings, theme: v })}
            options={[
              { value: 'dark', label: t('settings.dark'), icon: <Moon size={14} /> },
              { value: 'light', label: t('settings.light'), icon: <Sun size={14} /> },
              { value: 'system', label: t('settings.system'), icon: <Monitor size={14} /> },
            ]}
          />
        </Row>
      </Card>

      <Card title={t('settings.sec.translation')}>
        <Row title={t('settings.defaultDirection')} help={t('settings.defaultDirectionHelp')}>
          <Segmented<Direction>
            label={t('settings.defaultDirection')}
            value={settings.direction}
            onChange={setDirection}
            options={DIRECTIONS.map((d) => ({ value: d, label: t(`dir.label.${d}` as Key) }))}
          />
        </Row>

        {(engine?.models ?? []).map((m) => {
          const ocr = engine?.ocr.find((o) => o.direction === m.id)
          const filesOk = m.files === 'ok'
          const busy = reloading === m.id || m.state === 'loading'
          const stateLabel =
            m.state === 'ready'
              ? t('state.ready')
              : m.state === 'failed'
                ? t('state.failed')
                : m.state === 'loading'
                  ? t('state.loading')
                  : t('state.notLoaded')
          const stateColor = m.state === 'ready' ? 'text-accent2' : m.state === 'failed' ? 'text-danger' : m.state === 'loading' ? 'text-warn' : 'text-muted'
          const offline = filesOk && m.state !== 'failed' && m.state !== 'loading'
          const check = checks[m.id]
          return (
            <div key={m.id} className="mt-4 rounded-xl border border-line p-3.5">
              <div className="mb-2 flex flex-wrap items-center gap-2">
                <span className="text-[13.5px] font-semibold">{t(`dir.label.${m.id}` as Key)}</span>
                <span className={`text-[12.5px] font-medium ${stateColor}`}>{stateLabel}</span>
                {offline && (
                  <span className="rounded-full bg-accent2/15 px-2 py-0.5 text-[11px] text-accent2">{t('settings.offlineReady')}</span>
                )}
              </div>
              <dl className="space-y-2 text-[13px]">
                <Line label={t('settings.modelFiles')}>
                  <span className={filesOk ? 'text-accent2' : 'text-danger'}>
                    {filesOk ? t('settings.filesOk') : t('settings.filesMissing')}
                  </span>
                </Line>
                <Line label={t('settings.modelSize')}>
                  <span dir="ltr">{mb(m.sizeBytes)}</span>
                </Line>
                <Line label={t('settings.modelLicense')}>
                  <span dir="ltr" className="break-words">
                    {m.upstream} · {m.license}
                  </span>
                </Line>
                <Line label={t('settings.ocrFor', { pair: t(`history.pair.${m.id}` as Key) })}>
                  <span className={ocr?.available ? 'text-accent2' : 'text-warn'}>
                    {ocr?.available
                      ? ocr.engine === 'tesseract'
                        ? t('settings.ocrReadyTess')
                        : t('settings.ocrReady', { lang: ocr.language ?? '' })
                      : '—'}
                  </span>
                </Line>
              </dl>

              {m.modelDir && (
                <div className="mt-3">
                  <div className="mb-1 text-[12px] text-muted">{t('settings.modelFolder')}</div>
                  <div
                    dir="ltr"
                    className="selectable max-w-full break-all rounded-lg border border-line bg-bg px-2.5 py-2 text-start text-[11.5px] leading-relaxed text-muted"
                  >
                    {m.modelDirDisplay || m.modelDir}
                  </div>
                </div>
              )}

              {ocr && !ocr.available && (
                <>
                  <p className="mt-3 text-[12px] leading-relaxed text-warn">
                    {m.id === 'he-ar' ? t('settings.ocrHeMissing') : t('settings.ocrMissing')}
                  </p>
                  {m.id === 'he-ar' && ocr.detail && (
                    <p dir="ltr" className="selectable mt-2 break-words text-[11.5px] text-faint">
                      {ocr.detail}
                    </p>
                  )}
                </>
              )}
              {m.state === 'failed' && m.error && (
                <p dir="ltr" className="selectable mt-3 break-words text-[11.5px] text-faint">
                  {m.error}
                </p>
              )}
              {check && (
                <p
                  role="status"
                  className={`mt-3 text-[12px] leading-relaxed ${check.state === 'ok' ? 'text-accent2' : 'text-danger'}`}
                >
                  {check.state === 'ok'
                    ? t('settings.verifyOk')
                    : check.state === 'missing'
                      ? t('settings.verifyMissing', { detail: check.detail })
                      : t('settings.verifyCorrupt', { detail: check.detail })}
                </p>
              )}

              <div className="mt-3 flex flex-wrap gap-2">
                <Button onClick={() => verify(m.id)} disabled={verifying === m.id}>
                  <ShieldCheck size={13} />
                  {verifying === m.id ? t('settings.verifying') : t('settings.verify')}
                </Button>
                <Button onClick={() => reload(m.id)} disabled={busy}>
                  <RefreshCw size={13} className={busy ? 'spin' : ''} />
                  {t('settings.reload')}
                </Button>
                <Button onClick={() => api.revealModelDir(m.id).catch((e) => toast(errString(e), 'error'))}>
                  <FolderOpen size={13} />
                  {t('settings.openFolder')}
                </Button>
                {m.modelDir && (
                  <Button onClick={() => copyPath(m)}>
                    <Copy size={13} />
                    {t('settings.copyPath')}
                  </Button>
                )}
              </div>
            </div>
          )
        })}

        <p className="mt-3 text-[12px] leading-relaxed text-muted">{t('settings.engineDesc')}</p>
        <p className="mt-1.5 text-[12px] leading-relaxed text-faint">{t('settings.offlineNote')}</p>
      </Card>

      <Card title={t('settings.sec.popup')}>
        <Row title={t('settings.popupSize')}>
          <Segmented<Settings['popupFontSize']>
            label={t('settings.popupSize')}
            value={settings.popupFontSize}
            onChange={(v) => v !== settings.popupFontSize && save({ ...settings, popupFontSize: v })}
            options={[
              { value: 's', label: t('settings.sizeS') },
              { value: 'm', label: t('settings.sizeM') },
              { value: 'l', label: t('settings.sizeL') },
            ]}
          />
        </Row>
        <Row title={t('settings.popupOpacity')}>
          <input
            type="range"
            min={60}
            max={100}
            step={5}
            value={opacity}
            onChange={(e) => onOpacity(Number(e.target.value))}
            aria-label={t('settings.popupOpacity')}
            className="w-36 accent-[var(--accent)]"
          />
          <span dir="ltr" className="w-10 text-end text-[12px] tabular-nums text-muted">
            {opacity}%
          </span>
        </Row>
        <Row title={t('settings.popupPinned')} help={t('settings.popupPinnedHelp')}>
          <Toggle
            label={t('settings.popupPinned')}
            checked={settings.popupPinned}
            onChange={(v) => save({ ...settings, popupPinned: v })}
          />
        </Row>
        <Row title={t('settings.popupRemember')} help={t('settings.popupRememberHelp')}>
          <Toggle
            label={t('settings.popupRemember')}
            checked={settings.popupRememberPosition}
            onChange={(v) => save({ ...settings, popupRememberPosition: v })}
          />
        </Row>
      </Card>

      <Card title={t('settings.sec.shortcuts')}>
        <ul className="divide-y divide-soft">
          {(Object.keys(slotKey) as Slot[]).map((slot) => {
            const value = settings[slot]
            const optional = slot === 'shortcutMain'
            return (
              <li key={slot} className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2 py-2.5 text-[13px]">
                <span className="min-w-0 flex-1 basis-40 font-medium">{t(slotKey[slot])}</span>
                <div className="flex flex-wrap items-center gap-2">
                  {recording === slot ? (
                    <span className="animate-pulse rounded-md border border-accent/70 px-2 py-1 text-[12px] text-accent">
                      {t('settings.pressKeys')}
                    </span>
                  ) : value ? (
                    <Shortcut value={value} />
                  ) : (
                    <span className="text-[12px] text-faint">{t('settings.notSet')}</span>
                  )}
                  <Button
                    onClick={() => {
                      setErr(null)
                      setRecording(recording === slot ? null : slot)
                    }}
                  >
                    {recording === slot ? t('settings.cancel') : value ? t('settings.change') : t('settings.set')}
                  </Button>
                  {optional && value && recording !== slot && (
                    <Button variant="ghost" onClick={() => save({ ...settings, shortcutMain: '' })}>
                      {t('settings.remove')}
                    </Button>
                  )}
                </div>
              </li>
            )
          })}
        </ul>
        <p className="mt-2 text-[11.5px] leading-relaxed text-faint">{t('settings.shortcutHelp')}</p>
      </Card>

      <Card title={t('settings.sec.privacy')}>
        <Row title={t('settings.historyEnable')} help={t('settings.historyHelp')}>
          <Toggle
            label={t('settings.historyEnable')}
            checked={settings.historyEnabled}
            onChange={(v) => save({ ...settings, historyEnabled: v })}
          />
        </Row>
        <Row title={t('settings.historyClear')} help={t('settings.historyClearHelp')}>
          <Button variant="danger" onClick={() => setConfirmClear(true)}>
            <Trash2 size={13} />
            {t('history.clearAll')}
          </Button>
        </Row>
        {!settings.historyEnabled && (
          <p className="border-t border-soft pt-3 text-xs leading-relaxed text-faint">{t('settings.historyKept')}</p>
        )}
        <p className="mt-2 text-xs leading-relaxed text-muted">{t('settings.privacy')}</p>
      </Card>

      <Card title={t('settings.sec.about')}>
        <div className="mb-3 flex items-center gap-3">
          <Logo size={34} />
          <div className="min-w-0 leading-tight">
            <div className="text-[14px] font-semibold">{t('app.name')}</div>
            <div className="text-[12px] text-muted" dir="ltr">
              {info ? t('settings.version', { v: info.version }) : ''}
            </div>
          </div>
        </div>
        <p className="text-[12px] leading-relaxed text-muted">{t('settings.aboutDesc')}</p>
        <p className="mt-2.5 text-[12px] leading-relaxed text-muted">{t('settings.quality')}</p>
        <p className="mt-2.5 text-[11.5px] leading-relaxed text-faint">{t('settings.tech')}</p>
        <p className="mt-2.5 text-[11px] leading-relaxed text-faint">{t('settings.notices')}</p>
      </Card>

      {confirmClear && (
        <Dialog
          danger
          title={t('history.clearTitle')}
          body={t('history.clearBody')}
          confirmLabel={t('history.clearConfirm')}
          cancelLabel={t('history.cancel')}
          onConfirm={clearHistory}
          onCancel={() => setConfirmClear(false)}
        />
      )}
    </div>
  )
}

function Line({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
      <dt className="shrink-0 text-muted">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </div>
  )
}
