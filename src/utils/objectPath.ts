import type { CellValue } from './dynamicRenderers'

export function getValueByPath(
  obj: Record<string, unknown>,
  path: string
): CellValue {
  const parts = path.split('.')
  let value: unknown = obj

  for (const part of parts) {
    if (value === null || value === undefined) {
      return undefined
    }

    if (typeof value !== 'object' || value === null) {
      return undefined
    }

    value = (value as Record<string, unknown>)[part]
  }

  return value as CellValue
}

