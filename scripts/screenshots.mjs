// Capture README screenshots from the UI running on mock data (see src/dev/mock.ts).
//
//   pnpm dev                       (in another terminal)
//   node scripts/screenshots.mjs
//
// Drives headless Microsoft Edge or Chrome over the DevTools protocol with a throwaway
// profile and writes 2x PNGs to docs/images/. Palette shots are cropped to the card and
// keep a transparent background. Needs Node 22+ (global WebSocket).
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "docs", "images");
const base = process.env.KAKU_DEV_URL ?? "http://localhost:1420";
const port = 9333;

const browsers = [
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
  "/usr/bin/microsoft-edge",
];
const browser = process.env.KAKU_BROWSER ?? browsers.find((b) => existsSync(b));
if (!browser) throw new Error("No Edge/Chrome found; set KAKU_BROWSER.");

const WINDOW = { width: 1000, height: 720 };
// The palette window is 340 px wide plus 6 px padding; extra room around it keeps the shadow.
const PALETTE = { width: 440, height: 640, palette: true };
const shots = [
  { name: "dashboard", url: "/?mock&tab=dashboard&theme=dark", ...WINDOW },
  { name: "dashboard-light", url: "/?mock&tab=dashboard&theme=light", ...WINDOW },
  { name: "provider", url: "/?mock&tab=provider&theme=dark", ...WINDOW },
  { name: "shortcuts", url: "/?mock&tab=shortcuts&theme=dark", ...WINDOW },
  { name: "history", url: "/?mock&tab=history&theme=dark", ...WINDOW },
  { name: "palette", url: "/popup.html?mock&scene=menu&theme=dark", ...PALETTE },
  { name: "palette-result", url: "/popup.html?mock&scene=result&theme=dark", ...PALETTE },
  { name: "palette-dictation", url: "/popup.html?mock&scene=dictation&theme=dark", ...PALETTE },
  // Social preview: composed from the palette shots above, so it goes last.
  { name: "og", url: "/docs/og/og.html", width: 1280, height: 640, scale: 1 },
];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function connect() {
  for (let i = 0; i < 50; i++) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      const page = targets.find((t) => t.type === "page");
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      /* not up yet */
    }
    await sleep(200);
  }
  throw new Error("browser did not start");
}

function client(wsUrl) {
  const ws = new WebSocket(wsUrl);
  let id = 0;
  const pending = new Map();
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data);
    if (msg.id && pending.has(msg.id)) {
      const { ok, fail } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) fail(new Error(msg.error.message));
      else ok(msg.result);
    }
  };
  const ready = new Promise((r) => (ws.onopen = r));
  const send = (method, params = {}) =>
    new Promise((ok, fail) => {
      const n = ++id;
      pending.set(n, { ok, fail });
      ws.send(JSON.stringify({ id: n, method, params }));
    });
  return { ready, send, close: () => ws.close() };
}

// Without the dev server every shot would be the browser's "can't reach this page".
try {
  await fetch(base);
} catch {
  console.error(`The dev server is not running at ${base}. Start it with \`pnpm dev\` first.`);
  process.exit(1);
}

mkdirSync(out, { recursive: true });
const profile = mkdtempSync(join(tmpdir(), "kaku-shots-"));
const proc = spawn(
  browser,
  [
    "--headless=new",
    "--disable-gpu",
    "--hide-scrollbars",
    "--no-first-run",
    "--disable-extensions",
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${profile}`,
    "about:blank",
  ],
  { stdio: "ignore" },
);

try {
  const cdp = client(await connect());
  await cdp.ready;
  for (const s of shots) {
    await cdp.send("Emulation.setDeviceMetricsOverride", {
      width: s.width,
      height: s.height,
      deviceScaleFactor: s.scale ?? 2,
      mobile: false,
    });
    await cdp.send("Emulation.setDefaultBackgroundColorOverride", s.palette ? { color: { r: 0, g: 0, b: 0, a: 0 } } : {});
    await cdp.send("Page.navigate", { url: `${base}${s.url}` });
    // Let tab switches, counters, charts and the palette scene finish animating.
    await sleep(3500);
    await cdp.send("Runtime.evaluate", { expression: "document.activeElement?.blur()" });
    await sleep(300);

    let clip;
    if (s.palette) {
      await cdp.send("Runtime.evaluate", {
        expression: "document.getElementById('root').style.cssText = 'width:352px;margin:24px 44px'",
      });
      await sleep(400);
      const { result } = await cdp.send("Runtime.evaluate", {
        expression:
          "(() => { const r = document.getElementById('root').firstElementChild.getBoundingClientRect(); return JSON.stringify([r.x, r.y, r.width, r.height]); })()",
        returnByValue: true,
      });
      const [x, y, w, h] = JSON.parse(result.value);
      // Room for the card's drop shadow.
      const pad = 30;
      clip = { x: x - pad, y: y - 12, width: w + pad * 2, height: h + pad + 18, scale: 1 };
    }
    const { data } = await cdp.send("Page.captureScreenshot", {
      format: "png",
      ...(clip ? { clip, captureBeyondViewport: true } : {}),
    });
    writeFileSync(join(out, `${s.name}.png`), Buffer.from(data, "base64"));
    console.log(`${s.name}.png`);
  }
  cdp.close();
} finally {
  proc.kill();
  await sleep(500);
  rmSync(profile, { recursive: true, force: true });
}
