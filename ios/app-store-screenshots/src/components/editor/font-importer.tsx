'use client'
import type { ReactNode, Ref } from 'react'
import { useEffect, useImperativeHandle, useRef } from 'react'
import { toast } from 'sonner'
import { cleanFontName, cleanImportedFont } from '@/lib/clean-imported-font'
import { isRecord } from '@/lib/project-validation'
import type { ImportedFont } from '@/lib/types'

export type FontImporterHandle = { open: () => void }

type Props = {
  onImported: (font: ImportedFont) => void
  onUploadingChange?: (uploading: boolean) => void
}

const MAX_FONT_BYTES = 16 * 1024 * 1024

async function fileToBase64(file: File): Promise<string> {
  const dataUrl = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.onloadend = () => {
      if (typeof reader.result === 'string') {
        resolve(reader.result)
      } else {
        reject(new Error('Unreadable file'))
      }
    }
    reader.onerror = reject
    reader.readAsDataURL(file)
  })
  return dataUrl.slice(dataUrl.indexOf(',') + 1)
}

// Hidden file input behind the toolbar's "Import font…" menu item. The server
// validates the file by its magic bytes and stores it under
// public/fonts/imported/<hash>.<ext>.
export function FontImporter({
  onImported,
  onUploadingChange,
  ref,
}: Props & { ref?: Ref<FontImporterHandle> }): ReactNode {
  const inputRef = useRef<HTMLInputElement>(null)
  const requestId = useRef(0)
  useEffect(
    () => () => {
      requestId.current += 1
    },
    []
  )
  useImperativeHandle(ref, () => ({ open: () => inputRef.current?.click() }), [])

  async function importFont(file: File) {
    const request = ++requestId.current
    onUploadingChange?.(true)
    try {
      if (file.size > MAX_FONT_BYTES) {
        throw new Error('Font file is too large (16MB maximum).')
      }
      await new FontFace('Import validation', await file.arrayBuffer()).load()
      if (request !== requestId.current) {
        return
      }
      const response = await fetch('/api/upload-font', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ data: await fileToBase64(file) }),
        signal: AbortSignal.timeout(15000),
      })
      const body: unknown = await response.json().catch(() => ({ ok: false }))
      if (request !== requestId.current) {
        return
      }
      const font = isRecord(body) && body.ok === true ? cleanImportedFont(body.font) : undefined
      if (!response.ok || !font) {
        const serverError = isRecord(body) && typeof body.error === 'string' ? body.error : ''
        throw new Error(serverError || 'Could not import that font.')
      }
      const name = cleanFontName(file.name.replace(/\.[^.]+$/, ''))
      onImported({ ...font, ...(name ? { name } : {}) })
      toast.success(`Imported ${name ?? 'font'}`)
    } catch (caught) {
      if (request !== requestId.current) {
        return
      }
      toast.error('Font import failed', {
        description: caught instanceof Error ? caught.message : 'Could not import that font.',
      })
    } finally {
      if (request === requestId.current) {
        onUploadingChange?.(false)
      }
    }
  }

  return (
    <input
      ref={inputRef}
      type="file"
      accept=".woff2,.woff,.ttf,.otf,font/woff2,font/woff,font/ttf,font/otf"
      className="sr-only"
      tabIndex={-1}
      aria-hidden
      onChange={(event) => {
        const file = event.target.files?.[0]
        if (file) {
          void importFont(file)
        }
        event.target.value = ''
      }}
    />
  )
}
