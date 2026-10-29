import { type CellValue, isCellValue } from './dynamicRenderers'
import { isRecord } from './isRecord'

export function getValueByPath(obj: Record<string, unknown>, path: string): CellValue {
  const parts = path.split('.')
  let value: unknown = obj

  for (const part of parts) {
    if (!isRecord(value)) {
      return undefined
    }

    value = value[part]
  }

  return isCellValue(value) ? value : undefined
}
