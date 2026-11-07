import { glob, readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })

const notProductCode = /(^|\/)(testing|fixtures)\/|\.test\.tsx?$|\.d\.ts$|^main\.tsx$/
const viteOnlyImport = /from '[^']+\?worker'/

test('every source module loads', async () => {
  for await (const path of glob('**/*.{ts,tsx}', { cwd: import.meta.dirname })) {
    if (notProductCode.test(path)) {
      continue
    }
    if (viteOnlyImport.test(await readFile(join(import.meta.dirname, path), 'utf8'))) {
      continue
    }
    await import(`./${path}`)
  }
})
