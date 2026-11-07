import { glob, readFile } from 'node:fs/promises'
import { join } from 'node:path'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })

const src = join(import.meta.dirname, '..', 'src')
const notProductCode = /(^|\/)(testing|fixtures)\/|\.test\.tsx?$|\.d\.ts$|^main\.tsx$/
const viteOnlyImport = /from '[^']+\?worker'/

for await (const path of glob('**/*.{ts,tsx}', { cwd: src })) {
  if (notProductCode.test(path)) {
    continue
  }
  if (viteOnlyImport.test(await readFile(join(src, path), 'utf8'))) {
    continue
  }
  await import(join(src, path))
}
