import { save } from '@tauri-apps/plugin-dialog'
import { writeFile, writeTextFile } from '@tauri-apps/plugin-fs'

import { isTauri } from '../services'
import { mobileService } from '../services/mobile'

import { isMobilePlatform } from './platform'

interface SaveFileType {
  name: string
  extension: string
  mimeType: string
}

export interface SavedFile {
  hint?: string
}

export const describeSave = (message: string, saved: SavedFile): string =>
  saved.hint ? `${message.replace(/\.?$/, '.')} ${saved.hint}` : message

export async function saveFile(
  fileName: string,
  content: string | Uint8Array<ArrayBuffer>,
  fileType: SaveFileType
): Promise<SavedFile | null> {
  if (isMobilePlatform()) {
    if (typeof content !== 'string') {
      throw new Error("This device can't export binary files")
    }

    await mobileService.saveExportFile(fileName, content)

    return { hint: 'Find it in Files > On My iPhone > Pipedash.' }
  }

  if (isTauri()) {
    const filePath = await save({
      defaultPath: fileName,
      filters: [{ name: fileType.name, extensions: [fileType.extension] }],
    })

    if (!filePath) {
      return null
    }

    if (typeof content === 'string') {
      await writeTextFile(filePath, content)
    } else {
      await writeFile(filePath, content)
    }

    return {}
  }

  const url = URL.createObjectURL(new Blob([content], { type: fileType.mimeType }))
  const link = document.createElement('a')

  link.href = url
  link.download = fileName
  link.click()
  URL.revokeObjectURL(url)

  return {}
}
