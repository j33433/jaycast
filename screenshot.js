// Usage: node screenshot.js [trail-slug] [output-name] [--local] [--mobile]
// trail-slug: quiet-waters | markham | camp-murphy (default: quiet-waters)
// output-name: output filename (default: jaycast.png)
// --local: capture the local dist/ build instead of the live site
// --mobile: 390x844 viewport, no transparent background (default: desktop hero capture)
//
// Examples:
//   node screenshot.js
//   node screenshot.js quiet-waters jaycast.png
//   node screenshot.js markham markham.png --local
//   node screenshot.js camp-murphy mobile.png --local --mobile

const { createServer } = require('http');
const { createReadStream, statSync } = require('fs');
const { extname, join } = require('path');
const puppeteer = require('puppeteer');

const trail = process.argv[2] || 'quiet-waters';
const outFile = process.argv[3] || 'jaycast.png';
const local = process.argv.includes('--local');
const mobile = process.argv.includes('--mobile');

const MIME = {
  '.html': 'text/html',
  '.js': 'application/javascript',
  '.wasm': 'application/wasm',
  '.css': 'text/css',
  '.svg': 'image/svg+xml',
  '.webp': 'image/webp',
  '.png': 'image/png',
  '.json': 'application/json',
  '.xml': 'application/xml',
  '.txt': 'text/plain',
};

function serve() {
  const dist = join(__dirname, 'dist');
  return new Promise((resolve) => {
    const srv = createServer((req, res) => {
      let p = req.url.split('?')[0];
      if (p === '/') p = '/index.html';
      const fp = join(dist, p);
      try {
        statSync(fp);
        const ext = extname(fp).toLowerCase();
        res.writeHead(200, { 'Content-Type': MIME[ext] || 'application/octet-stream' });
        createReadStream(fp).pipe(res);
      } catch {
        res.writeHead(404);
        res.end();
      }
    });
    srv.listen(0, '127.0.0.1', () => resolve(srv));
  });
}

(async () => {
  let srv = null;
  let url;
  if (local) {
    srv = await serve();
    url = `http://127.0.0.1:${srv.address().port}/?${trail}`;
  } else {
    url = `https://upload.bike/jaycast/?${trail}`;
  }
  if (!mobile) url += '&screenshot';

  const browser = await puppeteer.launch({ headless: true, args: ['--no-sandbox'] });
  const page = await browser.newPage();
  if (mobile) {
    await page.setViewport({ width: 390, height: 844 });
  } else {
    await page.setViewport({ width: 800, height: 900, deviceScaleFactor: 2 });
  }

  console.log(`Opening ${url} ...`);
  await page.goto(url, { waitUntil: 'networkidle0', timeout: 30000 });

  // Give the WASM app a moment to settle.
  await new Promise((r) => setTimeout(r, 1500));

  if (!mobile) {
    // Force dark theme (GitHub display).
    await page.evaluate(() => {
      const html = document.documentElement;
      if (html.getAttribute('data-theme') !== 'dark') {
        localStorage.setItem('jaycast:theme', 'dark');
        html.setAttribute('data-theme', 'dark');
        const meta = document.querySelector('meta[name="theme-color"]');
        if (meta) meta.setAttribute('content', '#1a1712');
      }
    });
    await new Promise((r) => setTimeout(r, 500));
  }

  if (mobile) {
    await page.screenshot({ path: outFile, fullPage: false });
  } else {
    await page.screenshot({ path: outFile, omitBackground: true, fullPage: true });
  }
  console.log(`Saved ${outFile}`);

  await browser.close();
  if (srv) srv.close();
})();
