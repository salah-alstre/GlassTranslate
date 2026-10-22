import { useCallback, useEffect, useRef, useState } from 'react'
import { api, on } from '../api'
import { useI18n } from '../i18n'

type Pt = { x: number; y: number }
const MIN_DRAG = 4 // CSS px; smaller drags are treated as a stray click

export default function Selector() {
  const { t } = useI18n()
  const [src, setSrc] = useState<string | null>(null)
  const [start, setStart] = useState<Pt | null>(null)
  const [cur, setCur] = useState<Pt | null>(null)
  const [tooSmall, setTooSmall] = useState(false)
  const gen = useRef(0)
  const imgRef = useRef<HTMLImageElement>(null)

  const reset = useCallback(() => {
    setSrc(null)
    setStart(null)
    setCur(null)
    setTooSmall(false)
  }, [])

  useEffect(() => {
    const offStart = on<number>('selector-start', (g) => {
      gen.current = g
      reset()
      setSrc(`http://gtshot.localhost/current?g=${g}`)
    })
    const offClear = on('selector-clear', reset)
    return () => {
      offStart()
      offClear()
    }
  }, [reset])

  // Tell the backend only once the frozen frame is really on screen, so the window is never shown blank.
  const onLoaded = useCallback(() => {
    const img = imgRef.current
    const g = gen.current
    if (!img) return
    img
      .decode()
      .catch(() => {})
      .then(() => requestAnimationFrame(() => requestAnimationFrame(() => api.selectorReady(g).catch(() => {}))))
  }, [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        api.selectionCancel().catch(() => {})
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const rect =
    start && cur
      ? {
          x: Math.min(start.x, cur.x),
          y: Math.min(start.y, cur.y),
          w: Math.abs(cur.x - start.x),
          h: Math.abs(cur.y - start.y),
        }
      : null

  const down = (e: React.PointerEvent) => {
    if (e.button !== 0 || !src) return
    e.currentTarget.setPointerCapture(e.pointerId)
    setTooSmall(false)
    setStart({ x: e.clientX, y: e.clientY })
    setCur({ x: e.clientX, y: e.clientY })
  }
  const move = (e: React.PointerEvent) => {
    if (start) setCur({ x: e.clientX, y: e.clientY })
  }
  const up = (e: React.PointerEvent) => {
    if (!start || e.button !== 0) return
    const x = Math.min(start.x, e.clientX)
    const y = Math.min(start.y, e.clientY)
    const w = Math.abs(e.clientX - start.x)
    const h = Math.abs(e.clientY - start.y)
    setStart(null)
    setCur(null)
    if (w < MIN_DRAG || h < MIN_DRAG) {
      setTooSmall(true) // zero-area: stay open so the user can simply try again
      return
    }
    api
      .selectionDone({ x, y, w, h, vw: window.innerWidth, vh: window.innerHeight })
      .catch(() => {})
  }

  return (
    <div
      className="fixed inset-0 select-none overflow-hidden bg-black"
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onContextMenu={(e) => {
        e.preventDefault()
        api.selectionCancel().catch(() => {})
      }}
    >
      {src && (
        <img
          ref={imgRef}
          src={src}
          alt=""
          draggable={false}
          onLoad={onLoaded}
          className="pointer-events-none absolute inset-0 h-full w-full"
        />
      )}
      {/* Dim layer: a uniform wash before dragging, a cut-out around the selection while dragging. */}
      {!rect && <div className="pointer-events-none absolute inset-0 bg-black/40" />}
      {rect && (
        <div
          className="pointer-events-none absolute rounded-[3px]"
          style={{
            left: rect.x,
            top: rect.y,
            width: rect.w,
            height: rect.h,
            boxShadow: '0 0 0 100vmax rgba(0,0,0,0.55)',
            border: '2px solid #22d3ee',
            outline: '1px solid rgba(52,211,153,0.55)',
            outlineOffset: 2,
          }}
        >
          <span
            dir="ltr"
            className="absolute -top-7 left-0 whitespace-nowrap rounded-md bg-black/80 px-2 py-0.5 text-[11px] font-medium text-[#22d3ee]"
            style={rect.y < 32 ? { top: 'auto', bottom: -28 } : undefined}
          >
            {Math.round(rect.w)} × {Math.round(rect.h)}
          </span>
        </div>
      )}
      {!rect && src && (
        <div className="pointer-events-none absolute left-1/2 top-8 -translate-x-1/2">
          <div
            className={`rounded-full border px-4 py-2 text-[13px] font-medium backdrop-blur ${
              tooSmall ? 'border-[#fbbf24]/60 bg-black/80 text-[#fbbf24]' : 'border-[#22d3ee]/50 bg-black/75 text-[#e6fffb]'
            }`}
          >
            {tooSmall ? t('error.selection_invalid') : t('selector.hint')}
          </div>
        </div>
      )}
    </div>
  )
}
