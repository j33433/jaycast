// Usage: node screenshot.js [trail-slug] [output-name]
// trail-slug: quiet-waters | markham | camp-murphy (default: quiet-waters)
// output-name: output filename (default: jaycast.png)
//
// Examples:
//   node screenshot.js
//   node screenshot.js quiet-waters jaycast.png
//   node screenshot.js markham markham.png

const puppeteer = require('puppeteer');

const trail = process.argv[2] || 'quiet-waters';
const outFile = process.argv[3] || 'jaycast.png';

const url = `https://upload.bike/jaycast/?${trail}&screenshot`;

(async () => {
  const browser = await puppeteer.launch();
  const page = await browser.newPage();
  await page.setViewport({ width: 800, height: 900, deviceScaleFactor: 2 });

  console.log(`Opening ${url} ...`);
  await page.goto(url, { waitUntil: 'networkidle0' });

  // Give the WASM app a moment to settle.
  await new Promise(r => setTimeout(r, 1500));

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
  await new Promise(r => setTimeout(r, 500));

  await page.screenshot({ path: outFile, omitBackground: true, fullPage: true });
  console.log(`Saved ${outFile}`);

  await browser.close();
})();
