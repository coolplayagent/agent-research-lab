/** Decorative navigation symbols; accessible names belong to adjacent locale text. */
const paths = {
  topic: "M9 3h6M10 3v6l-5 9a2 2 0 0 0 1.8 3h10.4a2 2 0 0 0 1.8-3l-5-9V3M8 14h8M10 18h.01M14 17h.01",
  people: "M12 3v3M10 3h4M6 7h12a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2ZM1 12v4M23 12v4M8 12h.01M16 12h.01M9 16h6",
  applications: "M3 3h7v7H3ZM14 3h7v7h-7ZM3 14h7v7H3ZM14 14h7v7h-7Z",
};
export function icon(name: keyof typeof paths): SVGSVGElement {
  const ns = "http://www.w3.org/2000/svg", svg = document.createElementNS(ns, "svg"), path = document.createElementNS(ns, "path");
  for (const [key, value] of Object.entries({ viewBox: "0 0 24 24", "aria-hidden": "true", focusable: "false", class: "ui-icon", fill: "none", stroke: "currentColor", "stroke-width": "1.6", "stroke-linecap": "round", "stroke-linejoin": "round" })) svg.setAttribute(key, value);
  path.setAttribute("d", paths[name]); svg.append(path);
  return svg;
}
