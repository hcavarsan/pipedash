import { test } from '@e2e-dev/web';
import type { Browser } from '@e2e-dev/web';
import { expect } from 'e2e';
import { readFile, readdir, stat } from 'node:fs/promises';
import JSZip from 'jszip';
import sharp from 'sharp';

// iPhone Duo and App Store creative targets, plus the issue 41 export regression.

type State = Record<string, any>;
const slide = (id: string, layout = 'no-device', extra: State = {}) => ({ id, layout, screenshot: '', label: { en: 'FEATURE' }, headline: { en: id }, ...extra });
const project = (device: string, slidesByDevice: State, extra: State = {}): State => ({
  schemaVersion: 3, appName: 'Target Army', themeId: 'clean-light', connectedCanvas: true, locales: ['en'], locale: 'en',
  device, orientation: device.endsWith('-landscape') ? 'landscape' : 'portrait', slidesByDevice, ...extra,
});

async function mock(browser: Browser, initial: State) {
  const store = { state: initial };
  await browser.route('**/api/project', async route => {
    if (route.request.method === 'POST') store.state = JSON.parse(route.request.postData!);
    await route.fulfill({ json: { ok: true, state: store.state }, headers: { etag: '"mock"' } });
  });
  return store;
}

async function eventually(check: () => unknown | Promise<unknown>, timeout = 10_000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    try { await check(); return; } catch (error) {
      if (Date.now() > deadline) throw error;
      await new Promise(resolve => setTimeout(resolve, 200));
    }
  }
}

// Downloads in the same minute share a name across tests; take the newest.
async function zipFrom(path: string) {
  const matches = (await readdir('.e2e/artifacts', { recursive: true })).filter(file => file.endsWith(path));
  await expect(matches.length).toBeGreaterThan(0);
  const dated = await Promise.all(matches.map(async file => ({ file, at: (await stat(`.e2e/artifacts/${file}`)).mtimeMs })));
  dated.sort((a, b) => b.at - a.at);
  return JSZip.loadAsync(await readFile(`.e2e/artifacts/${dated[0].file}`));
}

const EXACT: Record<string, [number, number][]> = {
  'duo-outer': [[1398, 2034]],
  'duo-outer-landscape': [[2034, 1398]],
  'duo-inner': [[2007, 2853]],
  'duo-inner-landscape': [[2853, 2007]],
  'creative-universal': [[5244, 2950]],
  'creative-header': [[3840, 1646]],
  'creative-search': [[3840, 2560], [1920, 1280]],
};

for (const [device, sizes] of Object.entries(EXACT)) {
  test(`${device} exports exact opaque RGB PNGs`, { timeout: 240_000 }, async ({ app, browser, screen }) => {
    await mock(browser, project(device, { [device]: [slide('One', 'hero'), slide('Two')] }));
    await app.open('/');
    await expect(screen.getByRole('textbox', 'Headline')).toHaveValue('One');
    const download = await browser.waitForDownload(() => screen.getByRole('button', 'Export bundle', { exact: true }).tap(), { timeout: 200_000 });
    const zip = await zipFrom(download.path);
    const pngs = Object.values(zip.files).filter(file => file.name.endsWith('.png'));
    await expect(pngs).toHaveLength(sizes.length * 2);
    for (const file of pngs) {
      const match = file.name.match(new RegExp(`^ios/${device}/(\\d+)x(\\d+)/en/0[12]-(hero|no-device)\\.png$`))!;
      await expect(match).toBeTruthy();
      const bytes = await file.async('nodebuffer');
      const [w, h] = [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
      await expect(sizes.some(([sw, sh]) => sw === w && sh === h)).toBe(true);
      await expect([Number(match[1]), Number(match[2])]).toEqual([w, h]);
      // Colour type 2: 24-bit RGB, no alpha channel.
      await expect(bytes[25]).toBe(2);
      await expect((await sharp(bytes).metadata()).hasAlpha).toBe(false);
    }
  });
}

test('duo orientations keep independent decks and placements', async ({ app, browser, screen }) => {
  const placed = { device: { x: 120, y: 900, width: 700, height: 1000, rotation: 4 } };
  const store = await mock(browser, project('duo-outer', {
    'duo-outer': [slide('Portrait copy', 'hero', { transforms: placed })],
    'duo-outer-landscape': [slide('Landscape copy', 'split-landscape')],
    'duo-inner': [slide('Inner copy', 'hero')],
  }));
  await app.open('/');
  const headline = screen.getByRole('textbox', 'Headline');
  await expect(headline).toHaveValue('Portrait copy');
  await expect(screen.getByRole('combobox', 'Device')).toContainText('iPhone Duo outer');

  await screen.getByRole('combobox', 'Orientation').tap();
  await screen.getByRole('option', 'Landscape', { exact: true }).tap();
  await expect(headline).toHaveValue('Landscape copy');
  await headline.fill('Landscape edited');

  // Changing display keeps the orientation being worked in.
  await screen.getByRole('combobox', 'Device').tap();
  await screen.getByRole('option', 'iPhone Duo inner', { exact: true }).tap();
  await expect(screen.getByRole('combobox', 'Orientation')).toContainText('Landscape');
  await screen.getByRole('combobox', 'Orientation').tap();
  await screen.getByRole('option', 'Portrait', { exact: true }).tap();
  await expect(headline).toHaveValue('Inner copy');

  await screen.getByRole('combobox', 'Device').tap();
  await screen.getByRole('option', 'iPhone Duo outer', { exact: true }).tap();
  await expect(headline).toHaveValue('Portrait copy');
  await eventually(async () => {
    await expect(store.state.device).toBe('duo-outer');
    await expect(store.state.orientation).toBe('portrait');
    await expect(store.state.slidesByDevice['duo-outer-landscape'][0].headline.en).toBe('Landscape edited');
    await expect(store.state.slidesByDevice['duo-outer'][0].transforms).toEqual(placed);
    await expect(store.state.slidesByDevice['duo-outer'][0].headline.en).toBe('Portrait copy');
  });

  // Reload restores the same deck and orientation from the saved project.
  await browser.reload();
  await expect(screen.getByRole('textbox', 'Headline')).toHaveValue('Portrait copy');
  await expect(screen.getByRole('combobox', 'Orientation')).toContainText('Portrait');
});

test('legacy v2 projects gain the new decks without changing existing ones', async ({ app, browser, screen }) => {
  const legacy: State = { ...project('iphone', { iphone: [slide('Legacy', 'hero', { transforms: { caption: { x: 10, y: 20, width: 900, height: 400 } } })] }), schemaVersion: 2 };
  const store = await mock(browser, legacy);
  await app.open('/');
  await expect(screen.getByRole('textbox', 'Headline')).toHaveValue('Legacy');
  await screen.getByRole('textbox', 'App name').fill('Legacy migrated');
  await eventually(async () => {
    await expect(store.state.schemaVersion).toBe(3);
    const [kept] = store.state.slidesByDevice.iphone;
    await expect([kept.id, kept.layout, kept.headline, kept.transforms]).toEqual(['Legacy', 'hero', { en: 'Legacy' }, legacy.slidesByDevice.iphone[0].transforms]);
    for (const device of Object.keys(EXACT)) await expect(store.state.slidesByDevice[device].length).toBeGreaterThan(0);
  });
});

test('universal creative shows safe area guides and placement previews that never export', async ({ app, browser, screen }) => {
  await mock(browser, project('creative-universal', { 'creative-universal': [slide('Brand line', 'split-landscape')] }));
  await app.open('/');
  await expect(browser.locator('main [data-creative-guide]')).toHaveCount(1);
  await expect(screen.getByText('Art safe area', { exact: true })).toBeVisible();
  await expect(browser.locator('[aria-label="Placement previews"] figure')).toHaveCount(2);
  // Only the editable canvas has guides: not the export canvas, thumbnails or previews.
  await expect(await browser.evaluate(() => document.querySelectorAll('[data-creative-guide]').length)).toBe(1);
});

// Issue 41: clearing a label or headline exported the editor hints ("LABEL",
// "Headline goes here") into the PNG. Exported empty copy must match copy that
// renders nothing, while the editable canvas still shows the hints.
test('cleared copy exports empty while the canvas keeps its editing hints', { timeout: 240_000 }, async ({ app, browser, screen }) => {
  const plain = { backdrop: 'solid', span: false, decoration: 'none', shadow: 0, glow: 0, tilt: 0, headlineWeight: 700, headlineScale: 1, headlineCase: 'as-typed', captionAlign: 'auto' };
  // A zero-width space is not :empty, so it renders as genuinely blank copy.
  await mock(browser, project('watchos', {
    watchos: [
      slide('Cleared', 'no-device', { label: {}, headline: {} }),
      slide('Blank', 'no-device', { label: { en: '​' }, headline: { en: '​' } }),
    ],
  }, { scene: plain, connectedCanvas: false }));
  await app.open('/');
  await expect(browser.locator('main [contenteditable="plaintext-only"]').first()).toBeVisible();
  const hint = await browser.evaluate(() => {
    const fields = [...document.querySelectorAll('main [contenteditable="plaintext-only"]')].slice(0, 2);
    return fields.map(el => getComputedStyle(el, '::before').content);
  });
  await expect(hint).toEqual(['"LABEL"', '"Headline goes here"']);

  const download = await browser.waitForDownload(() => screen.getByRole('button', 'Export bundle', { exact: true }).tap(), { timeout: 200_000 });
  const zip = await zipFrom(download.path);
  const raw = async (name: string) => sharp(await zip.file(name)!.async('nodebuffer')).raw().toBuffer();
  const cleared = await raw('ios/watchos/422x514/en/01-no-device.png');
  const blank = await raw('ios/watchos/422x514/en/02-no-device.png');
  let differing = 0;
  for (let i = 0; i < cleared.length; i++) if (Math.abs(cleared[i] - blank[i]) > 24) differing++;
  await expect(differing).toBeLessThan(cleared.length * 0.001);
});

// On iPhone Duo, "Two devices" is the same phone folded and open: the back
// device is the other display, so it never borrows the front display's capture.
test('duo folded + open pairs the other display and never reuses the front capture', { timeout: 240_000 }, async ({ app, browser, screen }) => {
  // Routes fulfil text only, so the inner-display capture is an SVG of that size.
  const front = '<svg xmlns="http://www.w3.org/2000/svg" width="2853" height="2007"><rect width="2853" height="2007" fill="#4f46e5"/></svg>';
  await browser.route('**/screenshots/duo/front.svg', route => route.fulfill({ body: front, headers: { 'content-type': 'image/svg+xml' } }));
  await mock(browser, project('duo-inner-landscape', {
    'duo-inner-landscape': [slide('Pair', 'two-devices', { screenshot: '/screenshots/duo/front.svg', screenshotSecondary: '' })],
  }));
  await app.open('/');
  await expect(screen.getByRole('textbox', 'Headline')).toHaveValue('Pair');
  await expect(screen.getByRole('combobox', 'Layout')).toContainText('Folded + open');
  await expect(screen.getByText('iPhone Duo outer screenshot', { exact: true })).toBeVisible();
  // The front device shows its capture; only the closed phone waits for one.
  await eventually(async () => {
    await expect(await browser.evaluate(() => [...document.querySelectorAll('main')].map(m => m.textContent!.split('Drop a screenshot here').length - 1)[0])).toBe(1);
  });

  await screen.getByRole('button', 'Export bundle', { exact: true }).tap();
  await expect(screen.getByText('Export includes placeholder screenshots', { exact: true })).toBeVisible();
  await expect(browser.locator('[data-sonner-toast]').first()).toContainText('1 screen will export with an empty device.');
});

// The two Duo displays differ in shape by only 2.35 %, so an outer capture on the
// inner deck must still be flagged (and letterboxed), whether it is served from a
// URL or saved inline in the project, where its size is only known after decoding.
const outerCapture = '<svg xmlns="http://www.w3.org/2000/svg" width="1398" height="2034"><rect width="1398" height="2034" fill="#0ea5e9"/></svg>';
for (const [how, screenshot] of [
  ['served', '/screenshots/duo/outer.svg'],
  ['inline', `data:image/svg+xml;base64,${Buffer.from(outerCapture).toString('base64')}`],
] as const) {
  test(`an outer-display capture on the inner deck is flagged (${how})`, async ({ app, browser, screen }) => {
    await browser.route('**/screenshots/duo/outer.svg', route => route.fulfill({ body: outerCapture, headers: { 'content-type': 'image/svg+xml' } }));
    await mock(browser, project('duo-inner', { 'duo-inner': [slide('Wrong display', 'hero', { screenshot })] }));
    await app.open('/');
    await expect(screen.getByRole('textbox', 'Headline')).toHaveValue('Wrong display');
    await expect(screen.getByText(/isn't 2007 × 2853/)).toBeVisible();
    // Letterboxed, never cropped.
    await eventually(async () => {
      const fits = await browser.evaluate(() => [...document.querySelectorAll('main img')].map(i => getComputedStyle(i).objectFit));
      await expect(fits).toContain('contain');
    });
  });
}
