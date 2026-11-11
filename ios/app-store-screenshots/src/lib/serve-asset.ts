import { promises as fs } from 'node:fs'
import path from 'node:path'
import { NextResponse } from 'next/server'

export const isMissingFile = (error: unknown): boolean =>
  error instanceof Error && 'code' in error && error.code === 'ENOENT'

/** Next's production public-file index excludes uploads created after startup. */
export async function serveAsset(
  directory: string,
  filename: string,
  types: Record<string, string>
): Promise<NextResponse> {
  const extension = /^[a-f0-9]{16}\.([a-z0-9]+)$/.exec(filename)?.[1]
  const contentType = extension !== undefined && Object.hasOwn(types, extension) ? types[extension] : undefined
  if (contentType === undefined) {
    return new NextResponse(null, { status: 404 })
  }
  try {
    const bytes = await fs.readFile(path.join(process.cwd(), 'public', directory, filename))
    return new NextResponse(new Uint8Array(bytes), {
      headers: {
        'Content-Type': contentType,
        'Content-Length': String(bytes.length),
        'Cache-Control': 'public, max-age=31536000, immutable',
        'X-Content-Type-Options': 'nosniff',
      },
    })
  } catch (error) {
    if (isMissingFile(error)) {
      return new NextResponse(null, { status: 404 })
    }
    return NextResponse.json({ ok: false, error: 'Asset could not be read' }, { status: 500 })
  }
}
