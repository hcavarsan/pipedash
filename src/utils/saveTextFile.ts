import { save } from '@tauri-apps/plugin-dialog'
import { writeTextFile } from '@tauri-apps/plugin-fs'

import { isTauri } from '../services'

export async function saveTextFile(fileName: string, content: string): Promise<boolean> {
  if (isTauri()) {
    const filePath = await save({ defaultPath: fileName, filters: [{ name: 'TOML File', extensions: ['toml'] }] })

    if (!filePath) {
      return false
    }
    await writeTextFile(filePath, content)

    return true
  }

  const url = URL.createObjectURL(new Blob([content], { type: 'application/toml' }))
  const link = document.createElement('a')

  link.href = url
  link.download = fileName
  link.click()
  URL.revokeObjectURL(url)

  return true
}
