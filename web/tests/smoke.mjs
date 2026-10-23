// The stage under node: the hero builds, draws what the native build
// draws, and counts a click on its own button only.
//
//   node web/tests/smoke.mjs <native-golden.rgba>
//
// Run through web/test.sh, which writes the golden first.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import createStage from "../dist/ogham_stage.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const golden = process.argv[2] ? readFileSync(process.argv[2]) : null;
const hero = readFileSync(here + "hero.ogh", "utf8");

const m = await createStage();
const fn = (name, ret, args) => m.cwrap(name, ret, args);
const api = {
  new: fn("stage_new", null, ["number", "number"]),
  setSource: fn("stage_set_source", "number", ["number", "number"]),
  error: fn("stage_error", "number", []),
  outPtr: fn("stage_out_ptr", "number", []),
  resize: fn("stage_resize", null, ["number", "number", "number"]),
  pointer: fn("stage_pointer", null, ["number", "number", "number", "number"]),
  wheel: fn("stage_wheel", "number", ["number", "number", "number", "number"]),
  frame: fn("stage_frame", "number", ["number"]),
  pixels: fn("stage_pixels_ptr", "number", []),
  width: fn("stage_pixel_width", "number", []),
  height: fn("stage_pixel_height", "number", []),
};

function withString(text, f) {
  const bytes = new TextEncoder().encode(text);
  const ptr = m._malloc(bytes.length || 1);
  m.HEAPU8.set(bytes, ptr);
  try {
    return f(ptr, bytes.length);
  } finally {
    m._free(ptr);
  }
}
const out = (len) => new TextDecoder().decode(m.HEAPU8.slice(api.outPtr(), api.outPtr() + len));
const pixels = () => m.HEAPU8.slice(api.pixels(), api.pixels() + api.width() * api.height() * 4);

let failures = 0;
function check(ok, what) {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures++;
}

withString("", api.new);
api.resize(600, 400, 2);
const built = withString(hero, api.setSource);
check(built === 0, `hero builds${built ? ": " + out(api.error()) : ""}`);
check((api.frame(16) & 1) === 1, "first frame repaints");
check(api.width() === 1200 && api.height() === 800, "1200x800 device pixels at 2x");

const first = pixels();
if (golden) {
  let differing = 0;
  let worst = 0;
  for (let i = 0; i < first.length; i++) {
    const d = Math.abs(first[i] - golden[i]);
    if (d) differing++;
    worst = Math.max(worst, d);
  }
  check(golden.length === first.length && differing === 0,
    `matches the native render (${differing} channels differ, worst ${worst})`);
}

// Rectangle sums over logical rects, as the native test does.
function changed(a, b, x, y, w, h) {
  let sum = 0;
  const pw = api.width();
  for (let py = y * 2; py < (y + h) * 2; py++)
    for (let px = x * 2; px < (x + w) * 2; px++)
      for (let c = 0; c < 4; c++) {
        const i = (py * pw + px) * 4 + c;
        sum += Math.abs(a[i] - b[i]);
      }
  return sum;
}
for (const kind of [0, 1, 2]) api.pointer(kind, 300, 144, 0);
api.frame(16);
const after = pixels();
check(changed(first, after, 190, 122, 220, 44) > 0, "the clicked button redraws");
check(changed(first, after, 190, 178, 220, 44) === 0, "its neighbours do not");

// The wheel is the document's only while a scroll container moves;
// otherwise the page scrolls. Positive dy is towards the top.
const list = `let main = fn () {
  Flex {
    style: { width: 300, height: 100, direction: "column", overflow: "scroll" },
    children: [Flex { style: { width: "grow", height: 250 } }],
  }
};`;
check(withString(list, api.setSource) === 0, "a scroll list builds");
api.frame(16);
check(api.wheel(50, 50, 0, 40) === 0, "at its top, a wheel up is the page's");
check(api.wheel(50, 50, 0, -40) === 1, "a wheel down is the list's");
check(api.wheel(50, 50, 0, -400) === 1, "to its end");
check(api.wheel(50, 50, 0, -40) === 0, "past which the page's again");
check(api.wheel(350, 50, 0, -40) === 0, "and outside it, always the page's");

check(withString("let main = fn () { Flex {", api.setSource) === 1, "a bad edit is refused");
check(out(api.error()).length > 0, "and says why");

process.exit(failures ? 1 : 0);
