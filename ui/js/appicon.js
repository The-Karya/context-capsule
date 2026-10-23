// context-capsule — ui: an app's real icon where the host has one, else the glyph — the same box either way, so
// nothing moves when the icon arrives.

import { icon } from "./icons.js";
import { call } from "./host.js";
import { makeIconCache } from "./core/appicons.js";

const cache = makeIconCache((apps) => call("apps.icons", { apps }));

/** `icon(glyph, cls)` that shows the app's icon instead once the host sends it. */
export function appIcon(app, bundle, glyph, cls) {
  const el = icon(glyph, cls);
  const show = (url) => {
    if (!url) return;
    const img = document.createElement("img");
    img.src = url;
    img.alt = "";
    img.draggable = false;
    el.replaceChildren(img);
    el.classList.add("real");
  };
  const hit = cache.want(app, bundle, show);
  if (hit) show(hit);
  return el;
}
