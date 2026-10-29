export class RowHeights {
  readonly size: number
  private readonly values: Float64Array
  private readonly tree: Float64Array
  private readonly top: number

  constructor(size: number, init: number | ((index: number) => number)) {
    this.size = size
    this.values = new Float64Array(size)
    this.tree = new Float64Array(size + 1)

    for (let index = 0; index < size; index += 1) {
      const height = typeof init === 'number' ? init : init(index)

      this.values[index] = height
      this.tree[index + 1] = height
    }

    for (let node = 1; node <= size; node += 1) {
      const parent = node + (node & -node)

      if (parent <= size) {
        this.tree[parent] = (this.tree[parent] ?? 0) + (this.tree[node] ?? 0)
      }
    }

    let top = 1

    while (top * 2 <= size) {
      top *= 2
    }

    this.top = size === 0 ? 0 : top
  }

  get(index: number): number {
    return this.values[index] ?? 0
  }

  set(index: number, height: number): void {
    const delta = height - (this.values[index] ?? 0)

    if (delta === 0) {
      return
    }

    this.values[index] = height

    for (let node = index + 1; node <= this.size; node += node & -node) {
      this.tree[node] = (this.tree[node] ?? 0) + delta
    }
  }

  prefix(index: number): number {
    let sum = 0

    for (let node = Math.min(index, this.size); node > 0; node -= node & -node) {
      sum += this.tree[node] ?? 0
    }

    return sum
  }

  total(): number {
    return this.prefix(this.size)
  }

  indexAt(y: number): number {
    if (this.size === 0 || y <= 0) {
      return 0
    }

    if (y >= this.total()) {
      return this.size - 1
    }

    let position = 0
    let remaining = y

    for (let step = this.top; step > 0; step >>= 1) {
      const next = position + step

      const value = this.tree[next] ?? 0

      if (next <= this.size && value <= remaining) {
        position = next
        remaining -= value
      }
    }

    return Math.min(position, this.size - 1)
  }
}
