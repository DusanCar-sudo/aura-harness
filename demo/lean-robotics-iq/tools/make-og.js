// Render the branded social card (1200x630) from tools/og-card.html.
// Requires a local server serving the repo root, e.g.:
//   python3 -m http.server 8191 --bind 127.0.0.1 --directory .
// Run with the globally installed Playwright:
//   NODE_PATH=/home/dusan/.local/lib/node_modules node tools/make-og.js
const { chromium } = require('playwright');
const path = require('path');

(async () => {
  const browser = await chromium.launch({
    headless: true,
    executablePath: process.env.CHROME_PATH || '/home/dusan/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome',
    args: ['--no-sandbox', '--disable-gpu'],
  });
  const ctx = await browser.newContext({ viewport: { width: 1200, height: 630 }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  await page.goto('http://127.0.0.1:8191/tools/og-card.html', { waitUntil: 'networkidle' });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(250);
  await page.locator('.card').screenshot({
    path: path.join(__dirname, '..', 'assets', 'img', 'og-card.jpg'),
    type: 'jpeg',
    quality: 88,
  });
  await browser.close();
  console.log('assets/img/og-card.jpg written');
})();
