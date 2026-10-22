import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { AlertTriangle, Check, Info, X } from 'lucide-react'
import { useI18n } from '../i18n'

export type ToastKind = 'success' | 'error' | 'info'
interface ToastItem {
  id: number
  kind: ToastKind
  text: string
}

type Push = (text: string, kind?: ToastKind) => void
const Ctx = createContext<Push>(() => {})

/** In-app toasts only — never a Windows notification. */
export function ToastProvider({ children }: { children: ReactNode }) {
  const { t } = useI18n()
  const [items, setItems] = useState<ToastItem[]>([])
  const seq = useRef(0)
  const timers = useRef(new Map<number, number>())

  const dismiss = useCallback((id: number) => {
    setItems((l) => l.filter((x) => x.id !== id))
    const h = timers.current.get(id)
    if (h) window.clearTimeout(h)
    timers.current.delete(id)
  }, [])

  const push = useCallback<Push>(
    (text, kind = 'success') => {
      const id = ++seq.current
      // Keep at most three on screen; identical consecutive messages are collapsed.
      setItems((l) => [...l.filter((x) => x.text !== text), { id, kind, text }].slice(-3))
      timers.current.set(id, window.setTimeout(() => dismiss(id), kind === 'error' ? 5200 : 2600))
    },
    [dismiss],
  )

  useEffect(() => {
    const m = timers.current
    return () => m.forEach((h) => window.clearTimeout(h))
  }, [])

  const value = useMemo(() => push, [push])

  return (
    <Ctx.Provider value={value}>
      {children}
      <div
        className="pointer-events-none fixed inset-x-0 bottom-3 z-50 flex flex-col items-center gap-2 px-4"
        role="region"
        aria-label="Notifications"
      >
        {items.map((it) => (
          <div
            key={it.id}
            role={it.kind === 'error' ? 'alert' : 'status'}
            className="toast pointer-events-auto flex max-w-full items-start gap-2 rounded-xl border border-line bg-raised px-3 py-2 text-[13px] text-fg shadow-lg"
          >
            <span
              className={`mt-0.5 shrink-0 ${it.kind === 'error' ? 'text-danger' : it.kind === 'info' ? 'text-muted' : 'text-accent'}`}
            >
              {it.kind === 'error' ? (
                <AlertTriangle size={15} aria-hidden />
              ) : it.kind === 'info' ? (
                <Info size={15} aria-hidden />
              ) : (
                <Check size={15} aria-hidden />
              )}
            </span>
            <span className="min-w-0 break-words">{it.text}</span>
            <button
              type="button"
              onClick={() => dismiss(it.id)}
              aria-label={t('toast.dismiss')}
              className="-me-1 ms-1 shrink-0 rounded-md p-0.5 text-faint hover:text-fg"
            >
              <X size={13} aria-hidden />
            </button>
          </div>
        ))}
      </div>
    </Ctx.Provider>
  )
}

export function useToast(): Push {
  return useContext(Ctx)
}
