export function installLogViewerLayout(): () => void {
  const saved = {
    clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight'),
    clientWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth'),
    rect: Object.getOwnPropertyDescriptor(Element.prototype, 'getBoundingClientRect'),
    observer: Object.getOwnPropertyDescriptor(globalThis, 'ResizeObserver'),
  }

  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 400 })
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, get: () => 800 })
  Object.defineProperty(Element.prototype, 'getBoundingClientRect', {
    configurable: true,
    value: () => new DOMRect(0, 0, 100, 20),
  })
  Object.defineProperty(globalThis, 'ResizeObserver', {
    configurable: true,
    value: class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  })

  const put = (target: object, name: string, descriptor: PropertyDescriptor | undefined) => {
    if (descriptor) {
      Object.defineProperty(target, name, descriptor)
    } else {
      Reflect.deleteProperty(target, name)
    }
  }

  return () => {
    put(HTMLElement.prototype, 'clientHeight', saved.clientHeight)
    put(HTMLElement.prototype, 'clientWidth', saved.clientWidth)
    put(Element.prototype, 'getBoundingClientRect', saved.rect)
    put(globalThis, 'ResizeObserver', saved.observer)
  }
}
