'use client'
import { type RefObject, useEffect } from 'react'
import type { Slide } from '@/lib/types'

// Text fields and contenteditable text keep their native undo/redo, selection,
// and deletion. Sliders, colour pickers, checkboxes and buttons have no text
// history of their own, so the editor's shortcuts still apply while they're focused.
const NON_TEXT_INPUT_TYPES: Record<string, true> = {
  range: true,
  color: true,
  checkbox: true,
  radio: true,
  button: true,
  submit: true,
  reset: true,
  file: true,
}

// Radix menus/dialogs and dnd-kit own keyboard navigation while active.
const KEYBOARD_OWNER_SELECTOR =
  '[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"], [role="combobox"], [role="tablist"], [aria-roledescription="sortable"]'

type Shortcuts = {
  exportInProgress: RefObject<boolean>
  slides: Slide[]
  activeSlide: Slide | null
  selectSlide: (id: string) => void
  clearSelection: () => void
  duplicateSlide: (id: string) => void
  deleteSlide: (id: string) => void
  undo: () => void
  redo: () => void
}

function isTextEditable(target: HTMLElement | null): boolean {
  if (!target) {
    return false
  }
  if (target.isContentEditable || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT') {
    return true
  }
  return target instanceof HTMLInputElement && !NON_TEXT_INPUT_TYPES[target.type]
}

function moveActive(e: KeyboardEvent, s: Shortcuts, delta: 1 | -1): void {
  e.preventDefault()
  const idx = s.activeSlide ? s.slides.findIndex((slide) => slide.id === s.activeSlide?.id) : -1
  const next = s.slides[Math.min(s.slides.length - 1, Math.max(0, idx + delta))]
  if (next) {
    s.selectSlide(next.id)
  }
}

function handleSlideKey(e: KeyboardEvent, s: Shortcuts): void {
  const mod = e.metaKey || e.ctrlKey
  const key = e.key.toLowerCase()
  if (e.key === 'ArrowDown' || (e.key === 'j' && !mod)) {
    moveActive(e, s, 1)
  } else if (e.key === 'ArrowUp' || (e.key === 'k' && !mod)) {
    moveActive(e, s, -1)
  } else if (key === 'd' && mod && s.activeSlide) {
    e.preventDefault()
    s.duplicateSlide(s.activeSlide.id)
  } else if ((e.key === 'Backspace' || e.key === 'Delete') && mod && s.activeSlide) {
    e.preventDefault()
    s.deleteSlide(s.activeSlide.id)
  }
}

function handleKeydown(e: KeyboardEvent, s: Shortcuts): void {
  const target = e.target instanceof HTMLElement ? e.target : null
  if (s.exportInProgress.current || e.defaultPrevented || target?.closest(KEYBOARD_OWNER_SELECTOR)) {
    return
  }
  if (e.key === 'Escape') {
    s.clearSelection()
    return
  }

  const inTextField = isTextEditable(target)
  const key = e.key.toLowerCase()
  // Undo/redo belongs to the text field while one is focused.
  if ((e.metaKey || e.ctrlKey) && !e.altKey && (key === 'z' || key === 'y')) {
    if (!inTextField) {
      e.preventDefault()
      if (key === 'y' || e.shiftKey) {
        s.redo()
      } else {
        s.undo()
      }
    }
    return
  }

  // Arrow keys, deletion etc. keep their native meaning inside any input.
  const inControl = inTextField || target?.tagName === 'INPUT' || target?.tagName === 'SELECT'
  if (!inControl && s.slides.length > 0) {
    handleSlideKey(e, s)
  }
}

export function useEditorShortcuts({
  exportInProgress,
  slides,
  activeSlide,
  selectSlide,
  clearSelection,
  duplicateSlide,
  deleteSlide,
  undo,
  redo,
}: Shortcuts): void {
  useEffect(() => {
    const shortcuts = {
      exportInProgress,
      slides,
      activeSlide,
      selectSlide,
      clearSelection,
      duplicateSlide,
      deleteSlide,
      undo,
      redo,
    }
    const onKey = (e: KeyboardEvent) => handleKeydown(e, shortcuts)
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [exportInProgress, slides, activeSlide, selectSlide, clearSelection, duplicateSlide, deleteSlide, undo, redo])
}
