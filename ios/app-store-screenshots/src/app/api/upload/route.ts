import { createHash } from 'node:crypto'
import { promises as fs } from 'node:fs'
import path from 'node:path'
import { NextResponse } from 'next/server'
import sharp from 'sharp'
import { isRecord } from '@/lib/project-validation'
import { decodeBase64, readJsonBody } from '@/lib/request-body'
import { rejectCrossSiteWrite, sniffImageType } from '@/lib/request-guard'
import { writeAsset } from '@/lib/write-asset'

export const dynamic = 'force-dynamic'

const UPLOAD_DIR_REL = path.join('public', 'screenshots', 'uploaded')
const PUBLIC_PREFIX = '/screenshots/uploaded'

const MIME_EXT: Record<string, string> = {
  'image/png': 'png',
  'image/jpeg': 'jpg',
  'image/jpg': 'jpg',
}

function parseDataUrl(dataUrl: string): { mime: string; bytes: Buffer } | null {
  const m = /^data:([^;]+);base64,(.+)$/.exec(dataUrl)
  if (!m) {
    return null
  }
  const [, rawMime, data] = m
  if (rawMime === undefined || data === undefined) {
    return null
  }
  const bytes = decodeBase64(data)
  return bytes ? { mime: rawMime.toLowerCase(), bytes } : null
}

export async function POST(req: Request): Promise<NextResponse> {
  // This route WRITES A FILE to disk. See lib/request-guard.ts.
  const blocked = rejectCrossSiteWrite(req)
  if (blocked) {
    return NextResponse.json({ ok: false, error: blocked.error }, { status: blocked.status })
  }
  const input = await readJsonBody(req, 12 * 1024 * 1024)
  if (input.response) {
    return input.response
  }
  const dataUrl = isRecord(input.value) ? input.value.dataUrl : undefined
  if (typeof dataUrl !== 'string' || !dataUrl) {
    return NextResponse.json({ ok: false, error: 'Missing dataUrl' }, { status: 400 })
  }
  const parsed = parseDataUrl(dataUrl)
  if (!parsed) {
    return NextResponse.json({ ok: false, error: 'Unsupported data URL' }, { status: 400 })
  }
  const ext = MIME_EXT[parsed.mime]
  if (!ext) {
    return NextResponse.json({ ok: false, error: `Unsupported mime: ${parsed.mime}` }, { status: 400 })
  }
  // The declared MIME comes from the caller-written data URL, so it decides
  // the stored extension. Require the bytes to actually be that image type.
  const sniffed = sniffImageType(parsed.bytes)
  if (!sniffed || MIME_EXT[sniffed] !== ext) {
    return NextResponse.json({ ok: false, error: 'Content does not match declared image type' }, { status: 400 })
  }
  if (parsed.bytes.byteLength > 8 * 1024 * 1024) {
    return NextResponse.json({ ok: false, error: 'Image too large (>8MB)' }, { status: 413 })
  }

  try {
    // Decode pixels too: valid headers alone still accept truncated images.
    await sharp(parsed.bytes, { limitInputPixels: 64 * 1024 * 1024, failOn: 'warning' }).stats()
  } catch {
    return NextResponse.json({ ok: false, error: 'Image is corrupt or exceeds 64 megapixels' }, { status: 400 })
  }

  const hash = createHash('sha1').update(parsed.bytes).digest('hex').slice(0, 16)
  const filename = `${hash}.${ext}`
  const absDir = path.join(process.cwd(), UPLOAD_DIR_REL)
  const absFile = path.join(absDir, filename)

  try {
    await fs.mkdir(absDir, { recursive: true })
    await writeAsset(absFile, parsed.bytes)
    return NextResponse.json({ ok: true, path: `${PUBLIC_PREFIX}/${filename}` })
  } catch (e) {
    return NextResponse.json({ ok: false, error: e instanceof Error ? e.message : String(e) }, { status: 500 })
  }
}
