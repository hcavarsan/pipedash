export function installRequestSubmitShim(): () => void {
  const nativeRequestSubmit = HTMLFormElement.prototype.requestSubmit

  HTMLFormElement.prototype.requestSubmit = function requestSubmit(
    this: HTMLFormElement,
    submitter?: HTMLElement | null
  ): void {
    if (submitter) {
      nativeRequestSubmit.call(this, submitter)

      return
    }

    const probe = document.createElement('button')

    probe.type = 'submit'
    this.append(probe)
    try {
      nativeRequestSubmit.call(this, probe)
    } finally {
      probe.remove()
    }
  }

  return () => {
    HTMLFormElement.prototype.requestSubmit = nativeRequestSubmit
  }
}
