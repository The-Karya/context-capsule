// context-capsule — ui: the window's icons, drawn like the Mac's SF Symbols (thin outlines in the label colour)

const S = 'fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"';

const paths = {
  house: `<path ${S} d="M2.2 7.6 8 2.6l5.8 5"/><path ${S} d="M3.6 6.4v7.1h3.2V10h2.4v3.5h3.2V6.4"/>`,
  timeline: `<rect ${S} x="2.6" y="2.8" width="11.4" height="10.4" rx="2"/><path ${S} d="M2.6 6.3h11.4M2.6 9.7h11.4M.9 4.6h.1M.9 8h.1M.9 11.4h.1"/>`,
  doc: `<path ${S} d="M4 1.8h5.2L12.6 5.2v8.4a.9.9 0 0 1-.9.9H4a.9.9 0 0 1-.9-.9V2.7a.9.9 0 0 1 .9-.9Z"/><path ${S} d="M9 1.9v3.4h3.5M5.4 8.2h4.8M5.4 10.6h4.8"/>`,
  folder: `<path ${S} d="M1.8 4.3a1 1 0 0 1 1-1h3.4l1.4 1.5h5.6a1 1 0 0 1 1 1v6.9a1 1 0 0 1-1 1H2.8a1 1 0 0 1-1-1Z"/><path ${S} d="M1.8 6.6h12.4"/>`,
  gear: `<circle ${S} cx="8" cy="8" r="2.2"/><path ${S} d="M6.9 1.6h2.2l.4 1.8 1.2.6 1.6-1 1.6 1.6-1 1.6.5 1.2 1.8.4v2.2l-1.8.4-.5 1.2 1 1.6-1.6 1.6-1.6-1-1.2.5-.4 1.8H6.9l-.4-1.8-1.2-.5-1.6 1-1.6-1.6 1-1.6-.5-1.2-1.8-.4V6.9l1.8-.4.5-1.2-1-1.6 1.6-1.6 1.6 1 1.2-.6Z"/>`,
  help: `<circle ${S} cx="8" cy="8" r="6.4"/><path ${S} d="M6.2 6.3a1.9 1.9 0 1 1 2.6 1.8c-.5.2-.8.6-.8 1.1v.5"/><circle cx="8" cy="11.4" r=".75" fill="currentColor"/>`,
  clock: `<circle ${S} cx="8" cy="8" r="6.3"/><path ${S} d="M8 4.3V8l-2.4 1.8"/>`,
  search: `<circle ${S} cx="6.8" cy="6.8" r="4.6" stroke-width="1.4"/><path ${S} d="m10.2 10.2 4 4" stroke-width="1.5"/>`,
  chevronLeft: `<path ${S} stroke-width="1.6" d="M10 2.5 4.5 8l5.5 5.5"/>`,
  chevronRight: `<path ${S} stroke-width="1.6" d="M6 2.5 11.5 8 6 13.5"/>`,
  disclosure: `<path ${S} stroke-width="1.7" d="M5.5 2.8 10.7 8l-5.2 5.2"/>`,
  updown: `<path ${S} stroke-width="1.6" d="M4.5 6 8 2.5 11.5 6M4.5 10 8 13.5 11.5 10"/>`,
  ellipsis: `<circle cx="3.3" cy="8" r="1.05" fill="currentColor"/><circle cx="8" cy="8" r="1.05" fill="currentColor"/><circle cx="12.7" cy="8" r="1.05" fill="currentColor"/>`,
  ellipsisCircle: `<circle ${S} cx="8" cy="8" r="6.5"/><circle cx="5.1" cy="8" r=".85" fill="currentColor"/><circle cx="8" cy="8" r=".85" fill="currentColor"/><circle cx="10.9" cy="8" r=".85" fill="currentColor"/>`,
  window: `<rect ${S} x="1.6" y="3" width="12.8" height="10" rx="2"/><path ${S} d="M1.6 5.8h12.8M3.5 4.4h.01M5 4.4h.01M6.5 4.4h.01"/>`,
  mic: `<rect ${S} x="5.8" y="1.6" width="4.4" height="8" rx="2.2"/><path ${S} d="M3.6 7.6a4.4 4.4 0 0 0 8.8 0M8 12v2.4"/>`,
  keyboard: `<rect ${S} x="1.4" y="4" width="13.2" height="8.4" rx="1.6"/><path ${S} d="M4 6.6h.01M6.3 6.6h.01M8.6 6.6h.01M10.9 6.6h.01M4.8 10h6.4"/>`,
  eyeSlash: `<path ${S} d="M1.5 8s2.4-4.3 6.5-4.3S14.5 8 14.5 8 12.1 12.3 8 12.3 1.5 8 1.5 8Z"/><circle ${S} cx="8" cy="8" r="2"/><path ${S} d="m2.5 2.5 11 11"/>`,
  eye: `<path ${S} d="M1.5 8s2.4-4.3 6.5-4.3S14.5 8 14.5 8 12.1 12.3 8 12.3 1.5 8 1.5 8Z"/><circle ${S} cx="8" cy="8" r="2"/>`,
  share: `<path ${S} d="M5.6 6.2H4.4a.9.9 0 0 0-.9.9v6.2a.9.9 0 0 0 .9.9h7.2a.9.9 0 0 0 .9-.9V7.1a.9.9 0 0 0-.9-.9h-1.2M8 1.6v8M5.6 3.9 8 1.6l2.4 2.3"/>`,
  check: `<path ${S} stroke-width="1.6" d="m3 8.4 3.2 3.2L13 4.6"/>`,
  plus: `<path ${S} d="M8 2.8v10.4M2.8 8h10.4"/>`,
  viewfinder: `<path ${S} d="M1.8 5V3.4a1.6 1.6 0 0 1 1.6-1.6H5M11 1.8h1.6a1.6 1.6 0 0 1 1.6 1.6V5M14.2 11v1.6a1.6 1.6 0 0 1-1.6 1.6H11M5 14.2H3.4a1.6 1.6 0 0 1-1.6-1.6V11M5 5.8h6M5 8h6M5 10.2h4"/>`,
  waveform: `<path ${S} d="M1.5 7v2M3.8 5.5v5M6.1 3v10M8.4 5v6M10.7 2.2v11.6M13 5.5v5M15 7v2"/>`,
  link: `<path ${S} d="M6.6 9.4a2.6 2.6 0 0 0 3.7 0l2.5-2.5a2.6 2.6 0 0 0-3.7-3.7l-1 1"/><path ${S} d="M9.4 6.6a2.6 2.6 0 0 0-3.7 0L3.2 9.1a2.6 2.6 0 0 0 3.7 3.7l1-1"/>`,
  power: `<path ${S} d="M8 1.6v5.6"/><path ${S} d="M4.6 3.6a5.6 5.6 0 1 0 6.8 0"/>`,
  terminal: `<rect ${S} x="1.6" y="2.8" width="12.8" height="10.4" rx="2"/><path ${S} d="m4.4 6.2 2 1.8-2 1.8M8 10h3.4"/>`,
};

/** An inline SVG icon (16 × 16 view), aria-hidden: the text next to it says what it is. */
export function icon(name, cls = "") {
  const span = document.createElement("span");
  span.className = "ic" + (cls ? " " + cls : "");
  span.setAttribute("aria-hidden", "true");
  span.innerHTML = `<svg viewBox="0 0 16 16">${paths[name] || ""}</svg>`;
  return span;
}

/** Fills every `[data-icon]` in the static page. */
export function fillIcons(root = document) {
  for (const el of root.querySelectorAll("[data-icon]")) {
    el.setAttribute("aria-hidden", "true");
    el.innerHTML = `<svg viewBox="0 0 16 16">${paths[el.dataset.icon] || ""}</svg>`;
  }
}
