import { useEffect, useRef, type ComponentProps, type ReactNode } from 'react'

/** The GlassTranslate mark: a flat rounded tile with a "translate" glyph. */
export function Logo({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true" className="shrink-0">
      <rect x="2" y="2" width="60" height="60" rx="17" fill="var(--accent)" />
      <path
        d="M17 22h18M26 18v4m-7 0c1 6 5 11 11 14m4-14c-1 6-5 11-11 14"
        stroke="var(--accent-ink)"
        strokeWidth="3.4"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      />
      <path
        d="M33 46l7-16 7 16m-11-5h8"
        stroke="var(--accent-ink)"
        strokeWidth="3.4"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      />
    </svg>
  )
}

export function Card({ title, children, className = '' }: { title?: string; children: ReactNode; className?: string }) {
  return (
    <section className={`rounded-2xl border border-line bg-surface p-4 ${className}`}>
      {title && <h2 className="mb-3 text-[11px] font-semibold uppercase tracking-wider text-faint">{title}</h2>}
      {children}
    </section>
  )
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd
      dir="ltr"
      className="rounded-md border border-line bg-raised px-1.5 py-0.5 font-sans text-[11px] font-medium text-muted"
    >
      {children}
    </kbd>
  )
}

/** Renders "Ctrl+Alt+G" as separate key caps. */
export function Shortcut({ value }: { value: string }) {
  return (
    <span dir="ltr" className="inline-flex flex-wrap items-center gap-1">
      {value.split('+').map((k, i) => (
        <Kbd key={i}>{k}</Kbd>
      ))}
    </span>
  )
}

type BtnVariant = 'primary' | 'secondary' | 'ghost' | 'danger'
const variants: Record<BtnVariant, string> = {
  primary: 'btn-primary border border-transparent font-semibold',
  secondary: 'border border-line bg-raised text-fg hover:border-faint disabled:opacity-50',
  ghost: 'border border-transparent text-muted hover:bg-raised hover:text-fg disabled:opacity-50',
  danger: 'border border-danger/50 text-danger hover:bg-danger/10 disabled:opacity-50',
}

export function Button({
  variant = 'secondary',
  className = '',
  type = 'button',
  ...rest
}: ComponentProps<'button'> & { variant?: BtnVariant }) {
  return (
    <button
      type={type}
      className={`inline-flex items-center justify-center gap-1.5 rounded-lg px-3 py-1.5 text-[13px] transition-colors ${variants[variant]} ${className}`}
      {...rest}
    />
  )
}

export function IconButton({
  label,
  className = '',
  type = 'button',
  children,
  active,
  ...rest
}: ComponentProps<'button'> & { label: string; active?: boolean }) {
  return (
    <button
      type={type}
      aria-label={label}
      title={label}
      className={`inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg transition-colors hover:bg-raised hover:text-fg disabled:opacity-50 ${
        active ? 'text-accent' : 'text-muted'
      } ${className}`}
      {...rest}
    >
      {children}
    </button>
  )
}

export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean
  onChange: (v: boolean) => void
  label: string
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative h-6 w-11 shrink-0 rounded-full border transition-colors ${
        checked ? 'border-accent bg-accent' : 'border-line bg-raised'
      }`}
    >
      <span
        className={`absolute top-[3px] h-[18px] w-[18px] rounded-full transition-all ${
          checked ? 'bg-ink start-[22px]' : 'bg-muted start-[3px]'
        }`}
      />
    </button>
  )
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T
  options: { value: T; label: string; icon?: ReactNode }[]
  onChange: (v: T) => void
  label: string
}) {
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex rounded-lg border border-line bg-raised p-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          onClick={() => onChange(o.value)}
          className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1 text-[13px] transition-colors ${
            value === o.value ? 'bg-surface text-fg shadow-sm ring-1 ring-line' : 'text-muted hover:text-fg'
          }`}
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  )
}

/** A labelled settings row: text on the start side, control on the end side. Wraps at narrow widths. */
export function Row({
  title,
  help,
  children,
}: {
  title: string
  help?: string
  children?: ReactNode
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 py-2.5">
      <div className="min-w-0 flex-1 basis-48">
        <div className="text-[13px] font-medium text-fg">{title}</div>
        {help && <p className="mt-0.5 text-xs leading-relaxed text-faint">{help}</p>}
      </div>
      {children && <div className="flex max-w-full shrink-0 items-center gap-2">{children}</div>}
    </div>
  )
}

/** Modal confirm dialog with focus trap-lite, Esc to cancel and focus restore. */
export function Dialog({
  title,
  body,
  confirmLabel,
  cancelLabel,
  danger,
  onConfirm,
  onCancel,
}: {
  title: string
  body: string
  confirmLabel: string
  cancelLabel: string
  danger?: boolean
  onConfirm: () => void
  onCancel: () => void
}) {
  const cancelRef = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null
    cancelRef.current?.focus()
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation()
        onCancel()
      } else if (e.key === 'Tab') {
        // Keep focus inside the two buttons.
        const btns = Array.from(document.querySelectorAll<HTMLElement>('[data-dialog] button'))
        if (!btns.length) return
        const i = btns.indexOf(document.activeElement as HTMLElement)
        e.preventDefault()
        btns[(i + (e.shiftKey ? btns.length - 1 : 1)) % btns.length].focus()
      }
    }
    window.addEventListener('keydown', key, true)
    return () => {
      window.removeEventListener('keydown', key, true)
      prev?.focus?.()
    }
  }, [onCancel])

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/60 p-4">
      <div
        data-dialog
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="dlg-title"
        aria-describedby="dlg-body"
        className="anim-pop w-full max-w-sm rounded-2xl border border-line bg-surface p-5 shadow-2xl"
      >
        <h2 id="dlg-title" className="text-[15px] font-semibold">
          {title}
        </h2>
        <p id="dlg-body" className="mt-2 text-[13px] leading-relaxed text-muted">
          {body}
        </p>
        <div className="mt-5 flex flex-wrap justify-end gap-2">
          <Button ref={cancelRef} onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button variant={danger ? 'danger' : 'primary'} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  )
}
