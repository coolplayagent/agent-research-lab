/** Stable, local SVG identities, shared by chat, directories and the crystal. */
import { el } from "./dom.js";
const ns = "http://www.w3.org/2000/svg";
const colors = [["#237f78", "#e0f2ed"], ["#7270a8", "#eeecf8"], ["#aa7540", "#fbefde"], ["#4882a8", "#e4f0f8"], ["#a56783", "#f7e9f0"], ["#648a52", "#eaf3e2"]];
function shape(tag: string, attrs: Record<string, string | number>): SVGElement {
  const node = document.createElementNS(ns, tag);
  Object.entries(attrs).forEach(([key, value]) => node.setAttribute(key, String(value)));
  return node;
}
export function avatarSvg(id: string): SVGSVGElement {
  let hash = 0;
  for (const letter of id) hash = (Math.imul(hash, 31) + letter.codePointAt(0)!) >>> 0;
  const [ink, background] = colors[hash % colors.length];
  const picture = shape("svg", { viewBox: "0 0 40 40", "aria-hidden": "true", class: "identity-picture", color: ink }) as SVGSVGElement;
  picture.append(shape("rect", { width: 40, height: 40, rx: 12, fill: background }));
  if (id === "operator") {
    picture.append(shape("circle", { cx: 20, cy: 15, r: 6, fill: ink, opacity: .65 }), shape("path", { d: "M9 34c0-14 22-14 22 0", fill: ink, opacity: .4 }));
  } else {
    picture.append(shape("path", { d: "M10 35c0-9 20-9 20 0", fill: ink, opacity: .24 }), shape("rect", { x: 9, y: 7, width: 22, height: 22, rx: 9, fill: ink, opacity: .18 }), shape("circle", { cx: 16, cy: 17, r: 2, fill: ink }), shape("circle", { cx: 24, cy: 17, r: 2, fill: ink }), shape("path", { d: "M16 23q4 3 8 0", fill: "none", stroke: ink, "stroke-width": 1.5 }));
  }
  return picture;
}
export function avatar(id: string): HTMLSpanElement {
  const face = el("span", undefined, "avatar");
  face.setAttribute("aria-hidden", "true");
  face.append(avatarSvg(id));
  return face;
}
