import { save } from '@tauri-apps/plugin-dialog'
import { writeFile, writeTextFile } from '@tauri-apps/plugin-fs'
import { isTauri } from '../services/isTauri'
import { mobileService } from '../services/mobile'

import { isMobilePlatform } from './platform'

interface SaveFileType {
  name: string
  extension: string
  mimeType: string
}

export async function saveFile(
  fileName: string,
  content: string | Uint8Array<ArrayBuffer>,
  fileType: SaveFileType
): Promise<boolean> {
  if (isMobilePlatform()) {
    return (await mobileService.saveExportFile(fileName, content)).shared
  }

  if (isTauri()) {
    const filePath = await save({
      defaultPath: fileName,
      filters: [{ name: fileType.name, extensions: [fileType.extension] }],
    })

    if (!filePath) {
      return false
    }

    if (typeof content === 'string') {
      await writeTextFile(filePath, content)
    } else {
      await writeFile(filePath, content)
    }

    return true
  }

  const url = URL.createObjectURL(new Blob([content], { type: fileType.mimeType }))
  const link = document.createElement('a')

  link.href = url
  link.download = fileName
  link.click()
  URL.revokeObjectURL(url)

  return true
}
