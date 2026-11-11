'use client'
import { type ChangeEvent, type CSSProperties, type KeyboardEvent, type ReactNode, useState } from 'react'

type Props = {
  value: string
  editable?: boolean | undefined
  onChange?: ((v: string) => void) | undefined
  style?: CSSProperties | undefined
  multiline?: boolean | undefined
  placeholder?: string | undefined
  onFocus?: (() => void) | undefined
  emphasis?: boolean | undefined
  className?: string | undefined
}

const EMPHASIS = /(\*[^*\n]+\*)/g

function renderText(value: string, emphasis: boolean): ReactNode {
  if (!emphasis) {
    return value
  }
  let offset = 0
  return value.split(EMPHASIS).map((part) => {
    const key = offset
    offset += part.length
    if (part.length < 3 || !part.startsWith('*') || !part.endsWith('*')) {
      return part
    }
    return (
      <span key={key} className="midnight-lit">
        {part.slice(1, -1)}
      </span>
    )
  })
}

const stopDrag = (e: { stopPropagation: () => void }) => e.stopPropagation()

const focusAtEnd = (el: HTMLTextAreaElement | null) => {
  if (!el || document.activeElement === el) {
    return
  }
  el.focus()
  el.setSelectionRange(el.value.length, el.value.length)
}

export function EditableText({
  value,
  editable,
  onChange,
  style,
  multiline = false,
  placeholder,
  onFocus,
  emphasis = false,
  className,
}: Props): ReactNode {
  const [draft, setDraft] = useState<string | null>(null)

  const textStyle: CSSProperties = {
    outline: 'none',
    whiteSpace: multiline ? 'pre-wrap' : 'nowrap',
    cursor: editable ? 'text' : 'default',
    ...style,
  }

  if (!editable) {
    return (
      <div className={className} dir="auto" style={textStyle}>
        {renderText(value, emphasis)}
      </div>
    )
  }

  if (draft !== null) {
    const handleChange = (e: ChangeEvent<HTMLTextAreaElement>) => {
      const text = multiline ? e.target.value : e.target.value.replace(/\n/g, '')
      setDraft(text)
      onChange?.(text)
    }
    const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (!multiline && e.key === 'Enter') {
        e.preventDefault()
        e.currentTarget.blur()
      }
    }
    return (
      <textarea
        aria-label={placeholder ?? 'Text'}
        className={className}
        dir="auto"
        rows={1}
        value={draft}
        placeholder={placeholder}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        onBlur={() => setDraft(null)}
        onMouseDown={stopDrag}
        onPointerDown={stopDrag}
        ref={focusAtEnd}
        style={{
          display: 'block',
          width: '100%',
          margin: 0,
          padding: 0,
          border: 0,
          background: 'transparent',
          resize: 'none',
          overflow: 'hidden',
          fieldSizing: 'content',
          font: 'inherit',
          color: 'inherit',
          ...textStyle,
        }}
      />
    )
  }

  const startEditing = () => {
    setDraft(value)
    onFocus?.()
  }

  return (
    <button
      type="button"
      className={className}
      dir="auto"
      onClick={startEditing}
      onMouseDown={stopDrag}
      onPointerDown={stopDrag}
      style={{
        display: 'block',
        width: '100%',
        margin: 0,
        padding: 0,
        border: 0,
        background: 'transparent',
        font: 'inherit',
        color: 'inherit',
        textAlign: 'inherit',
        ...textStyle,
      }}
    >
      {value ? renderText(value, emphasis) : <span style={{ opacity: 0.4 }}>{placeholder}</span>}
    </button>
  )
}
