;(function () {
  var root = document.documentElement
  var scheme = 'dark'

  try {
    var stored = localStorage.getItem('mantine-color-scheme-value')

    if (stored === 'light' || stored === 'dark' || stored === 'auto') {
      scheme = stored
    }
  } catch (_) {
    scheme = 'dark'
  }

  if (scheme === 'auto') {
    scheme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  }

  var ios = /iPhone|iPod/.test(navigator.userAgent)

  root.setAttribute('data-mantine-color-scheme', scheme)

  if (ios) {
    root.setAttribute('data-ios', '')
  }

  root.style.backgroundColor = ios ? (scheme === 'dark' ? '#000' : '#f2f2f7') : scheme === 'dark' ? '#242424' : '#fff'
})()
