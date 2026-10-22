import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  ChevronDown,
  ChevronUp,
  ClipboardPaste,
  Copy,
  ExternalLink,
  History as HistoryIcon,
  Keyboard,
  ScanText,
  Search,
  Star,
  Trash2,
  X,
} from 'lucide-react'
import { api, directionOf, errString, on, type HistoryEntry, type Settings } from '../api'
import { errorMessage, useI18n, type Key } from '../i18n'
import { useToast } from './Toast'
import { Button, Dialog, IconButton } from './ui'

const sourceIcon = { screen: ScanText, clipboard: ClipboardPaste, manual: Keyboard } as const

export default function HistoryView({ settings, mode }: { settings: Settings; mode: 'all' | 'favorites' }) {
  const { t, lang } = useI18n()
  const toast = useToast()
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null)
  const [q, setQ] = useState('')
  const [confirmClear, setConfirmClear] = useState(false)

  const load = useCallback(() => {
    api.historyList().then(setEntries).catch(() => setEntries([]))
  }, [])

  useEffect(() => {
    load()
    return on('history-changed', load)
  }, [load])

  const fmt = useMemo(
    () => new Intl.DateTimeFormat(lang === 'ar' ? 'ar' : 'en', { dateStyle: 'medium', timeStyle: 'short' }),
    [lang],
  )

  const favOnly = mode === 'favorites'
  const base = useMemo(() => (entries ?? []).filter((e) => !favOnly || e.favorite), [entries, favOnly])
  const needle = q.trim().toLowerCase()
  const shown = useMemo(
    () =>
      needle
        ? base.filter((e) => e.original.toLowerCase().includes(needle) || e.translation.toLowerCase().includes(needle))
        : base,
    [base, needle],
  )

  const fail = (e: unknown) => toast(errorMessage(t, errString(e)), 'error')
  const copy = (text: string) =>
    api
      .copyText(text)
      .then(() => toast(t('toast.copied')))
      .catch(fail)

  const toggleFav = (e: HistoryEntry) =>
    api
      .historySetFavorite(e.id, !e.favorite)
      .then(() => toast(t(e.favorite ? 'toast.favRemoved' : 'toast.favAdded')))
      .catch(fail)

  const remove = (e: HistoryEntry) =>
    api
      .historyDelete(e.id)
      .then(() => toast(t('toast.entryDeleted')))
      .catch(fail)

  const clearAll = async () => {
    setConfirmClear(false)
    try {
      await api.historyClear()
      toast(t('toast.historyCleared'))
    } catch (e) {
      fail(e)
    }
  }

  const enable = () =>
    api.setSettings({ ...settings, historyEnabled: true }).then(() => toast(t('toast.settingsSaved'))).catch(fail)

  // History is off and there is nothing stored: explain instead of showing an empty list.
  if (!settings.historyEnabled && entries !== null && entries.length === 0) {
    return (
      <div className="anim-fade flex flex-1 flex-col items-center justify-center gap-3 rounded-2xl border border-dashed border-line px-6 py-12 text-center">
        <div className="grid h-12 w-12 place-items-center rounded-2xl bg-raised text-accent">
          <HistoryIcon size={22} />
        </div>
        <h2 className="text-[15px] font-semibold">{t('history.offTitle')}</h2>
        <p className="max-w-[320px] text-[12.5px] leading-relaxed text-muted">{t('history.offBody')}</p>
        <Button variant="primary" onClick={enable}>
          {t('history.enable')}
        </Button>
      </div>
    )
  }

  const emptyTitle: Key = favOnly ? 'history.emptyFav' : 'history.empty'
  const emptyHint: Key = favOnly ? 'history.emptyFavHint' : 'history.emptyHint'

  return (
    <div className="flex flex-1 flex-col gap-3">
      {!settings.historyEnabled && (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-warn/40 bg-warn/10 px-3.5 py-2.5 text-[12.5px]">
          <span className="min-w-0 flex-1 basis-48 leading-relaxed">{t('settings.historyKept')}</span>
          <Button onClick={enable}>{t('history.enable')}</Button>
        </div>
      )}

      <div className="flex items-center gap-2">
        <div className="relative min-w-0 flex-1">
          <Search size={15} className="pointer-events-none absolute start-3 top-1/2 -translate-y-1/2 text-faint" />
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder={t('history.search')}
            aria-label={t('history.search')}
            className="w-full rounded-xl border border-line bg-surface py-2 pe-8 ps-9 text-[13px] text-fg outline-none transition placeholder:text-faint focus:border-accent/70"
          />
          {q && (
            <button
              type="button"
              onClick={() => setQ('')}
              aria-label={t('main.clear')}
              className="absolute end-2 top-1/2 -translate-y-1/2 text-faint hover:text-fg"
            >
              <X size={14} />
            </button>
          )}
        </div>
        {!favOnly && entries && entries.length > 0 && (
          <Button variant="danger" onClick={() => setConfirmClear(true)}>
            <Trash2 size={14} />
            {t('history.clearAll')}
          </Button>
        )}
      </div>

      {entries === null ? (
        <div className="space-y-3" aria-busy="true">
          {[0, 1, 2].map((i) => (
            <div key={i} className="skeleton h-24 rounded-2xl" />
          ))}
        </div>
      ) : base.length === 0 ? (
        <div className="anim-fade flex flex-1 flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-line px-6 py-12 text-center">
          <div className="grid h-12 w-12 place-items-center rounded-2xl bg-raised text-accent">
            {favOnly ? <Star size={22} /> : <HistoryIcon size={22} />}
          </div>
          <h2 className="text-[15px] font-semibold">{t(emptyTitle)}</h2>
          <p className="max-w-[300px] text-[12.5px] leading-relaxed text-muted">{t(emptyHint)}</p>
        </div>
      ) : shown.length === 0 ? (
        <p className="py-10 text-center text-[13px] text-muted">{t('history.noMatch', { q: q.trim() })}</p>
      ) : (
        <>
          <div className="text-[11.5px] text-faint">{t('history.count', { n: String(base.length) })}</div>
          <ul className="space-y-3">
            {shown.map((e) => (
              <Item
                key={e.id}
                e={e}
                when={fmt.format(new Date(e.ts))}
                onCopyOriginal={() => copy(e.original)}
                onCopyTranslation={() => copy(e.translation)}
                onReopen={() => api.historyReopen(e.id).catch(fail)}
                onFav={() => toggleFav(e)}
                onDelete={() => remove(e)}
              />
            ))}
          </ul>
        </>
      )}

      {confirmClear && (
        <Dialog
          danger
          title={t('history.clearTitle')}
          body={t('history.clearBody')}
          confirmLabel={t('history.clearConfirm')}
          cancelLabel={t('history.cancel')}
          onConfirm={clearAll}
          onCancel={() => setConfirmClear(false)}
        />
      )}
    </div>
  )
}

function Item({
  e,
  when,
  onCopyOriginal,
  onCopyTranslation,
  onReopen,
  onFav,
  onDelete,
}: {
  e: HistoryEntry
  when: string
  onCopyOriginal: () => void
  onCopyTranslation: () => void
  onReopen: () => void
  onFav: () => void
  onDelete: () => void
}) {
  const { t } = useI18n()
  const [open, setOpen] = useState(false)
  const Src = sourceIcon[e.source]
  const long = e.original.length > 140 || e.translation.length > 140 || e.translation.includes('\n')

  return (
    <li className="anim-fade rounded-2xl border border-line bg-surface p-3.5">
      <div className="mb-2 flex items-center gap-2 text-[11.5px] text-faint">
        <span className="inline-flex items-center gap-1 rounded-full bg-raised px-2 py-0.5 font-medium text-muted">
          <Src size={12} />
          {t(`history.source.${e.source}` as Key)}
        </span>
        <span dir="ltr" className="rounded-full bg-raised px-2 py-0.5 font-medium text-accent">
          {t(`history.pair.${directionOf(e.sourceLang)}` as Key)}
        </span>
        {e.method === 'pivot' && (
          <span className="rounded-full bg-raised px-2 py-0.5 text-muted">{t('history.method.pivot')}</span>
        )}
        <span className="min-w-0 flex-1 truncate">{when}</span>
        <IconButton
          label={t(e.favorite ? 'history.unfavorite' : 'history.favorite')}
          active={e.favorite}
          onClick={onFav}
          aria-pressed={e.favorite}
        >
          <Star size={16} fill={e.favorite ? 'currentColor' : 'none'} />
        </IconButton>
      </div>

      <p
        dir={e.sourceLang === 'he' ? 'rtl' : 'ltr'}
        lang={e.sourceLang}
        className={`selectable break-words text-start text-[13px] leading-relaxed text-muted ${open ? 'whitespace-pre-wrap' : 'line-clamp-2'}`}
      >
        {e.original}
      </p>
      <p
        dir="rtl"
        lang="ar"
        className={`selectable mt-2 break-words font-arabic text-[16px] leading-[1.8] ${open ? 'whitespace-pre-wrap' : 'line-clamp-3'}`}
      >
        {e.translation}
      </p>

      <div className="mt-2.5 flex flex-wrap items-center gap-1 border-t border-soft pt-2">
        <IconButton label={t('history.copyTranslation')} onClick={onCopyTranslation}>
          <Copy size={15} />
        </IconButton>
        <IconButton label={t('history.copyOriginal')} onClick={onCopyOriginal}>
          <Copy size={15} className="opacity-60" />
        </IconButton>
        <IconButton label={t('history.reopen')} onClick={onReopen}>
          <ExternalLink size={15} />
        </IconButton>
        {long && (
          <IconButton label={t(open ? 'history.collapse' : 'history.expand')} onClick={() => setOpen(!open)} aria-expanded={open}>
            {open ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
          </IconButton>
        )}
        <IconButton label={t('history.delete')} onClick={onDelete} className="ms-auto hover:!text-danger">
          <Trash2 size={15} />
        </IconButton>
      </div>
    </li>
  )
}
