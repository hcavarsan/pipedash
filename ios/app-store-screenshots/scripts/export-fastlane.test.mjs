import assert from 'node:assert/strict'
import test from 'node:test'
import { deflateSync } from 'node:zlib'
import JSZip from 'jszip'
import { validateBundle, validateProject } from './export-fastlane.mjs'

function chunk(type, data = Buffer.alloc(0)) {
  const body = Buffer.concat([Buffer.from(type), data])
  let crc = 0xffffffff
  for (const byte of body) {
    crc ^= byte
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0)
  }
  const length = Buffer.alloc(4)
  length.writeUInt32BE(data.length)
  const checksum = Buffer.alloc(4)
  checksum.writeUInt32BE((crc ^ 0xffffffff) >>> 0)
  return Buffer.concat([length, body, checksum])
}

function png(width = 1320, height = 2868, color = 2) {
  const header = Buffer.alloc(13)
  header.writeUInt32BE(width)
  header.writeUInt32BE(height, 4)
  header[8] = 8
  header[9] = color
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(Buffer.alloc((width * 3 + 1) * height))),
    chunk('IEND'),
  ])
}

const image = png()
const prefix = 'ios/iphone/1320x2868/en/'
const slides = [{ layout: 'hero' }, { layout: 'no-device' }]
async function bundle(entries) {
  const zip = new JSZip()
  for (const [name, bytes] of entries) zip.file(name, bytes)
  return zip.generateAsync({ type: 'nodebuffer' })
}

test('selects only the largest size and preserves deck order', async () => {
  const zip = await bundle([
    [`${prefix}02-no-device.png`, image],
    ['ios/iphone/1284x2778/en/01-hero.png', image],
    [`${prefix}01-hero.png`, image],
  ])
  assert.deepEqual((await validateBundle(zip, 'iphone', slides)).map(file => file.name), ['01-iphone.png', '02-iphone.png'])
})

test('rejects incomplete, extra, and out-of-order target-size members', async () => {
  for (const entries of [
    [[`${prefix}01-hero.png`, image]],
    [[`${prefix}01-hero.png`, image], [`${prefix}02-no-device.png`, image], [`${prefix}03-hero.png`, image]],
    [[`${prefix}01-no-device.png`, image], [`${prefix}02-hero.png`, image]],
    [[`${prefix}01-hero.png`, image], [`${prefix}01-no-device.png`, image]],
  ]) await assert.rejects(validateBundle(await bundle(entries), 'iphone', slides), /incomplete|duplicate|order/)
})

test('rejects corrupt ZIPs, truncated PNGs, wrong dimensions, and non-RGB images', async () => {
  await assert.rejects(validateBundle(Buffer.from('not a zip'), 'iphone', slides))
  for (const bytes of [image.subarray(0, image.length - 1), png(1319), png(1320, 2868, 6)]) {
    const zip = await bundle([[`${prefix}01-hero.png`, bytes]])
    await assert.rejects(validateBundle(zip, 'iphone', [slides[0]]), /truncated|width|opaque RGB/)
  }
})

test('rejects unsafe member paths and unsupported bundle locales', async () => {
  for (const name of [`../${prefix}01-hero.png`, `${prefix.replace('/en/', '/../en/')}01-hero.png`, `${prefix.replace('/en/', '/de/')}01-hero.png`]) {
    await assert.rejects(validateBundle(await bundle([[name, image]]), 'iphone', [slides[0]]), /Unsafe|Unexpected/)
  }
})

test('rejects unsupported locales and decks outside App Store limits before launch', () => {
  const project = { locales: ['en'], locale: 'en', device: 'iphone', orientation: 'portrait', slidesByDevice: { iphone: slides, ipad: slides } }
  for (const locales of [['en', 'de'], ['../en'], ['en-US'], []]) {
    assert.throws(() => validateProject({ ...project, locales }), /Only the en locale/)
  }
  for (const deck of [[], Array.from({ length: 11 }, () => slides[0])]) {
    assert.throws(() => validateProject({ ...project, slidesByDevice: { ...project.slidesByDevice, ipad: deck } }), /1–10 slides/)
  }
})
