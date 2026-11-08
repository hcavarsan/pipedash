'use client'
import { Image as ImageIcon, Upload, X } from 'lucide-react'
import type { ReactNode, RefObject } from 'react'
import { useEffect, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { didFail, img, setImage } from '@/lib/image-cache'
import { resolveScreenshot } from '@/lib/locale'
import { PlainImg } from './plain-img'

type Props = {
  label: string
  value: string
  locale?: string
  onChange: (v: string) => void
}

type PickerError = { value: string; locale: string | undefined; message: string }

const ACCEPTED = ['image/png', 'image/jpeg']
class UploadRejectedError extends Error {}

async function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
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
}

function rejectionMessage(body: unknown): string {
  if (typeof body === 'object' && body !== null && 'error' in body && typeof body.error === 'string' && body.error) {
    return body.error
  }
  return 'Image upload was rejected'
}

function uploadedPathOf(body: unknown): string | null {
  if (typeof body !== 'object' || body === null || !('ok' in body) || !body.ok) {
    return null
  }
  return 'path' in body && typeof body.path === 'string' && body.path ? body.path : null
}

async function uploadDataUrl(dataUrl: string): Promise<string | null> {
  try {
    const resp = await fetch('/api/upload', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ dataUrl }),
      signal: AbortSignal.timeout(15000),
    })
    if (resp.status >= 400 && resp.status < 500 && resp.status !== 404) {
      const body: unknown = await resp.json().catch(() => ({}))
      throw new UploadRejectedError(rejectionMessage(body))
    }
    if (!resp.ok) {
      return null
    }
    return uploadedPathOf(await resp.json())
  } catch (error) {
    if (error instanceof UploadRejectedError) {
      throw error
    }
    return null
  }
}

async function decodeSize(dataUrl: string): Promise<{ w: number; h: number }> {
  const image = new Image()
  image.src = dataUrl
  await image.decode()
  if (image.naturalWidth * image.naturalHeight > 64 * 1024 * 1024) {
    throw new Error('too large')
  }
  return { w: image.naturalWidth, h: image.naturalHeight }
}

function previewSource(value: string, isData: boolean, resolvedValue: string): string {
  if (!value) {
    return ''
  }
  return isData ? value : img(resolvedValue)
}

function valueLabelOf(uploading: boolean, value: string, isData: boolean): string {
  if (uploading) {
    return 'saving…'
  }
  if (!value) {
    return 'drop image, or click Pick'
  }
  return isData ? 'uploaded image (not on disk)' : value.replace(/^.*\/(?=[^/]+\/[^/]+$)/, '…/')
}

function useFileDrop(onFile: (file: File) => void): { ref: RefObject<HTMLDivElement | null>; dragging: boolean } {
  const ref = useRef<HTMLDivElement>(null)
  const [dragging, setDragging] = useState(false)

  useEffect(() => {
    const zone = ref.current
    if (!zone) {
      return
    }
    const onDragOver = (event: DragEvent) => {
      event.preventDefault()
      setDragging(true)
    }
    const onDragLeave = (event: DragEvent) => {
      if (event.target === zone) {
        setDragging(false)
      }
    }
    const onDrop = (event: DragEvent) => {
      event.preventDefault()
      setDragging(false)
      const file = event.dataTransfer?.files?.[0]
      if (file) {
        onFile(file)
      }
    }
    zone.addEventListener('dragover', onDragOver)
    zone.addEventListener('dragleave', onDragLeave)
    zone.addEventListener('drop', onDrop)
    return () => {
      zone.removeEventListener('dragover', onDragOver)
      zone.removeEventListener('dragleave', onDragLeave)
      zone.removeEventListener('drop', onDrop)
    }
  }, [onFile])

  return { ref, dragging }
}

export function ScreenshotPicker({ label, value, locale, onChange }: Props): ReactNode {
  const requestId = useRef(0)
  useEffect(
    () => () => {
      requestId.current += 1
    },
    []
  )
  const inputRef = useRef<HTMLInputElement>(null)
  const [error, setError] = useState<PickerError | null>(null)
  const [uploading, setUploading] = useState(false)

  function fail(message: string) {
    setError({ value, locale, message })
  }

  async function handleFile(file: File) {
    const request = ++requestId.current
    setUploading(false)
    setError(null)
    if (!ACCEPTED.includes(file.type)) {
      fail('Use PNG or JPG (App Store rejects other formats)')
      return
    }
    if (file.size > 8 * 1024 * 1024) {
      fail('Image too large (>8MB)')
      return
    }
    let dataUrl: string
    try {
      dataUrl = await fileToDataUrl(file)
    } catch {
      if (request === requestId.current) {
        fail('Failed to read file')
      }
      return
    }
    if (request !== requestId.current) {
      return
    }
    let size: { w: number; h: number }
    try {
      size = await decodeSize(dataUrl)
    } catch {
      if (request === requestId.current) {
        fail('Image is corrupt or exceeds 64 megapixels')
      }
      return
    }
    if (request !== requestId.current) {
      return
    }
    // Try to persist to disk so the screenshot survives a git clone.
    // If the upload endpoint is unreachable (e.g. static export), fall back
    // to the inline data URI — still works in the current session.
    setUploading(true)
    let uploadedPath: string | null
    try {
      uploadedPath = await uploadDataUrl(dataUrl)
    } catch (uploadError) {
      if (request === requestId.current) {
        setUploading(false)
        fail(uploadError instanceof Error ? uploadError.message : 'Image upload failed')
      }
      return
    }
    if (request !== requestId.current) {
      return
    }
    setUploading(false)
    if (uploadedPath) {
      setImage(uploadedPath, dataUrl, size)
      onChange(uploadedPath)
    } else {
      setImage(dataUrl, dataUrl, size)
      onChange(dataUrl)
    }
  }

  const { ref: dropRef, dragging } = useFileDrop((file) => void handleFile(file))

  const hasValue = !!value
  const isData = hasValue && value.startsWith('data:')
  const resolvedValue = hasValue && !isData && locale ? resolveScreenshot(value, locale) : value
  const previewSrc = previewSource(value, isData, resolvedValue)
  // Only flag "image not found" when the path is a real URL that we tried and failed.
  const knownMissing = hasValue && !isData && didFail(resolvedValue)
  const matchingError = error && error.value === value && error.locale === locale ? error.message : null
  const errorMessage = matchingError ?? (knownMissing ? `Image not found at ${resolvedValue}` : null)

  return (
    <div className="space-y-1">
      <div
        ref={dropRef}
        className={`flex items-center gap-3 rounded-md border p-2 transition-colors ${
          dragging ? 'border-primary bg-accent ring-2 ring-primary/30' : 'border-input'
        }`}
      >
        <div className="flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-md border bg-muted">
          {previewSrc ? (
            <PlainImg
              src={previewSrc}
              alt=""
              className="h-full w-full object-cover"
              draggable={false}
              onError={() => fail('Image failed to load')}
            />
          ) : (
            <ImageIcon className="h-4 w-4 text-muted-foreground" />
          )}
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="truncate text-xs font-medium">{label}</span>
          <span className="truncate text-[10px] text-muted-foreground">
            {dragging ? 'Drop to upload' : valueLabelOf(uploading, value, isData)}
          </span>
        </div>
        <input
          ref={inputRef}
          type="file"
          accept="image/png,image/jpeg"
          className="hidden"
          onChange={async (e) => {
            const input = e.currentTarget
            const file = input.files?.[0]
            if (file) {
              await handleFile(file)
            }
            input.value = ''
          }}
        />
        <Button type="button" variant="outline" size="sm" className="h-8" onClick={() => inputRef.current?.click()}>
          <Upload className="h-3.5 w-3.5" />
          Pick
        </Button>
        {hasValue && (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={() => {
              requestId.current += 1
              setUploading(false)
              onChange('')
              setError(null)
            }}
            aria-label="Clear screenshot"
            title="Clear"
          >
            <X className="h-4 w-4" />
          </Button>
        )}
      </div>
      {errorMessage && <p className="text-[11px] text-destructive">{errorMessage}</p>}
    </div>
  )
}
