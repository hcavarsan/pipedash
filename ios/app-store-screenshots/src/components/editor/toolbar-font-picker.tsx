'use client'
import { Upload } from 'lucide-react'
import { type ReactNode, useRef } from 'react'
import { Select, SelectContent, SelectItem, SelectSeparator, SelectTrigger, SelectValue } from '@/components/ui/select'
import { IMPORTED_FONT_FAMILY, SCREENSHOT_FONTS } from '@/lib/fonts'
import type { ImportedFont, ScreenshotFontId } from '@/lib/types'
import { FontImporter, type FontImporterHandle } from './font-importer'

const IMPORT_FONT_ACTION = '__import-font__'

function isFontId(id: string): id is ScreenshotFontId {
  return Object.hasOwn(SCREENSHOT_FONTS, id)
}

type Props = {
  fontId: ScreenshotFontId
  setFontId: (v: ScreenshotFontId) => void
  importedFont: ImportedFont | undefined
  setImportedFont: (font: ImportedFont) => void
  importing: boolean
  setImporting: (importing: boolean) => void
  busy: boolean
}

export function ToolbarFontPicker({
  fontId,
  setFontId,
  importedFont,
  setImportedFont,
  importing,
  setImporting,
  busy,
}: Props): ReactNode {
  const fontImporter = useRef<FontImporterHandle>(null)
  // "Imported font" is only a choice once a file has actually been imported.
  const fontIds = Object.keys(SCREENSHOT_FONTS)
    .filter(isFontId)
    .filter((id) => id !== 'self-hosted' || !!importedFont)
  const fontLabel = (id: ScreenshotFontId) =>
    id === 'self-hosted' && importedFont?.name ? importedFont.name : SCREENSHOT_FONTS[id].name
  const changeFont = (value: string) => {
    if (value === IMPORT_FONT_ACTION) {
      fontImporter.current?.open()
    } else if (isFontId(value)) {
      setFontId(value)
    }
  }
  const fontOptions = fontIds.map((id) => {
    const fontFamily = id === 'self-hosted' ? `"${IMPORTED_FONT_FAMILY}", sans-serif` : SCREENSHOT_FONTS[id].family
    return (
      <SelectItem key={id} value={id}>
        <span style={{ fontFamily }}>{fontLabel(id)}</span>
      </SelectItem>
    )
  })

  return (
    <>
      <Select value={fontId} onValueChange={changeFont} disabled={busy || importing}>
        <SelectTrigger className="h-8 w-36 text-xs" title="Screenshot font" aria-label="Screenshot font">
          <SelectValue placeholder="Font">
            <span className="truncate">{importing ? 'Importing font…' : fontLabel(fontId)}</span>
          </SelectValue>
        </SelectTrigger>
        <SelectContent>
          {fontOptions}
          <SelectSeparator />
          <SelectItem value={IMPORT_FONT_ACTION}>
            <span className="flex items-center gap-1.5">
              <Upload className="h-3.5 w-3.5" />
              {importedFont ? 'Replace imported font…' : 'Import font…'}
            </span>
          </SelectItem>
        </SelectContent>
      </Select>
      <FontImporter ref={fontImporter} onImported={setImportedFont} onUploadingChange={setImporting} />
    </>
  )
}
