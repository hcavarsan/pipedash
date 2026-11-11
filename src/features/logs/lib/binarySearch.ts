export function partitionPoint(length: number, before: (index: number) => boolean): number {
  let low = 0
  let high = length

  while (low < high) {
    const mid = (low + high) >>> 1

    if (before(mid)) {
      low = mid + 1
    } else {
      high = mid
    }
  }

  return low
}
