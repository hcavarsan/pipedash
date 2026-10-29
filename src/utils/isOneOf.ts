export function isOneOf<const T extends string>(values: readonly T[], value: string | null | undefined): value is T {
  return values.some((candidate) => candidate === value)
}
