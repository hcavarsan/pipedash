export const BASE_PATH = new URL(document.baseURI).pathname

export const withBasePath = (path: string): string => (path.startsWith('/') ? `${BASE_PATH}${path.slice(1)}` : path)
