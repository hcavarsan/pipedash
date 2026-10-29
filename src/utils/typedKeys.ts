export function typedKeys<K extends string>(record: Record<K, unknown>): K[] {
  return Object.keys(record).filter((key): key is K => key in record)
}
