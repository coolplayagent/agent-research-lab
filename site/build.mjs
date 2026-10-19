import fs from 'node:fs';
import path from 'node:path';

process.on('uncaughtException', (error) => {
  process.stderr.write('Site generation failed: ' + error.message + '\n');
  process.exitCode = 1;
});

const [markedFile, destination, stylesheet, englishFile, chineseFile, ...inputs] = process.argv.slice(2);
if (!markedFile || !destination || !stylesheet || !englishFile || !chineseFile || !inputs.length) {
  throw new Error('usage: node build.mjs MARKED OUTPUT CSS EN_LOCALE ZH_LOCALE SOURCES...');
}
const { marked } = await import('data:text/javascript;base64,' +
  fs.readFileSync(markedFile).toString('base64'));
const translations = {
  en: JSON.parse(fs.readFileSync(englishFile, 'utf8')),
  'zh-CN': JSON.parse(fs.readFileSync(chineseFile, 'utf8')),
};
function sameShape(left, right, location = 'locale') {
  if (Array.isArray(left) || Array.isArray(right)) {
    if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length) {
      throw new Error('locale shape differs at ' + location);
    }
    left.forEach((value, index) => sameShape(value, right[index], location + '[' + index + ']'));
  } else if (left && typeof left === 'object' && right && typeof right === 'object') {
    if (JSON.stringify(Object.keys(left).sort()) !== JSON.stringify(Object.keys(right).sort())) {
      throw new Error('locale keys differ at ' + location);
    }
    for (const key of Object.keys(left)) sameShape(left[key], right[key], location + '.' + key);
  } else if (typeof left !== typeof right) {
    throw new Error('locale type differs at ' + location);
  }
}
sameShape(translations.en, translations['zh-CN']);
const root = 'https://coolplayagent.github.io/agent-research-lab/';
const repository = 'https://github.com/coolplayagent/agent-research-lab';
const release = repository + '/releases/tag/v0.1.1';
const sources = inputs.map((input) => path.posix.normalize(input.replaceAll(path.sep, '/')));
const sourceSet = new Set(sources);
const markdown = sources.filter((input) => input.endsWith('.md'));
const outputSet = new Set(markdown.map((input) => input.slice(0, -3) + '.html'));
for (const input of sources) {
  if (input.startsWith('../') || path.posix.isAbsolute(input)) {
    throw new Error('source leaves the workspace: ' + input);
  }
}
fs.mkdirSync(destination, { recursive: true });
fs.copyFileSync(stylesheet, path.join(destination, 'styles.css'));

function escapeHtml(value) {
  return String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&#39;');
}
function languageFor(input) {
  return input === 'README.md' || input.startsWith('docs/zh-CN/') ? 'zh-CN' : 'en';
}
function relativeTo(sourcePath, targetPath) {
  return path.posix.relative(path.posix.dirname(sourcePath), targetPath) ||
    path.posix.basename(targetPath);
}
function localLink(input, href) {
  if (href.startsWith('#') || /^(https?:|mailto:)/i.test(href)) return href;
  if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith('//') || href.startsWith('/')) {
    throw new Error('unsupported link in ' + input + ': ' + href);
  }
  const hash = href.indexOf('#');
  const plain = hash < 0 ? href : href.slice(0, hash);
  const fragment = hash < 0 ? '' : href.slice(hash);
  const resolved = path.posix.normalize(path.posix.join(
    path.posix.dirname(input), decodeURIComponent(plain)));
  if (resolved === '..' || resolved.startsWith('../') || !sourceSet.has(resolved)) {
    throw new Error('missing linked source in ' + input + ': ' + href);
  }
  const served = resolved.endsWith('.md') ? resolved.slice(0, -3) + '.html' : resolved;
  if (resolved.endsWith('.md') && !outputSet.has(served)) {
    throw new Error('linked document is not rendered: ' + resolved);
  }
  return relativeTo(input.slice(0, -3) + '.html', served) + fragment;
}
function plainHeading(text) {
  return text.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1').replace(/[\x60*_~]/g, '');
}
function slug(text) {
  return plainHeading(text).toLocaleLowerCase().replace(/[^\p{L}\p{N}\s-]/gu, '')
    .trim().replace(/\s+/g, '-').replace(/-+/g, '-') || 'section';
}
function pageShell(output, lang, title, content, description) {
  const tr = translations[lang];
  const css = relativeTo(output, 'styles.css');
  const home = relativeTo(output, lang === 'en' ? 'index.html' : 'index.zh-CN.html');
  const docs = relativeTo(output, 'docs/' + lang + '/00-index.html');
  const guide = relativeTo(output, 'docs/' + lang + '/guide/00-index.html');
  const design = relativeTo(output, 'docs/' + lang + '/design/00-index.html');
  let alternate = lang === 'en' ? 'docs/zh-CN/00-index.html' : 'docs/en/00-index.html';
  if (output.startsWith('docs/en/')) {
    const peer = output.replace(/^docs\/en\//, 'docs/zh-CN/');
    if (outputSet.has(peer)) alternate = peer;
  } else if (output.startsWith('docs/zh-CN/')) {
    const peer = output.replace(/^docs\/zh-CN\//, 'docs/en/');
    if (outputSet.has(peer)) alternate = peer;
  } else if (output === 'README.html') alternate = 'README.en.html';
  else if (output === 'README.en.html') alternate = 'README.html';
  else if (output === 'index.html') alternate = 'index.zh-CN.html';
  else if (output === 'index.zh-CN.html') alternate = 'index.html';
  const alternateHref = relativeTo(output, alternate);
  const canonical = root + output.split('/').map(encodeURIComponent).join('/');
  const nav = '<a href="' + escapeHtml(home) + '">' + escapeHtml(tr.home) + '</a>' +
    '<a href="' + escapeHtml(docs) + '">' + escapeHtml(tr.documentation) + '</a>' +
    '<a href="' + escapeHtml(guide) + '">' + escapeHtml(tr.guides) + '</a>' +
    '<a href="' + escapeHtml(design) + '">' + escapeHtml(tr.design) + '</a>' +
    '<a href="' + escapeHtml(release) + '">' + escapeHtml(tr.release) + '</a>';
  const sidebar = '<aside class="sidebar"><span class="eyebrow">' + escapeHtml(tr.browse) + '</span>' +
    '<a href="' + escapeHtml(docs) + '">' + escapeHtml(tr.documentation) + '</a>' +
    '<a href="' + escapeHtml(guide) + '">' + escapeHtml(tr.guides) + '</a>' +
    '<a href="' + escapeHtml(design) + '">' + escapeHtml(tr.design) + '</a>' +
    '<a href="' + escapeHtml(relativeTo(output, 'docs/' + lang + '/guide/01-quick-start.html')) + '">' +
    escapeHtml(tr.quickStart) + '</a><a href="' + escapeHtml(repository) + '">' +
    escapeHtml(tr.source) + '</a></aside>';
  return '<!doctype html>\n<html lang="' + lang + '"><head><meta charset="utf-8">' +
    '<meta name="viewport" content="width=device-width,initial-scale=1">' +
    '<meta name="description" content="' + escapeHtml(description) + '">' +
    '<meta http-equiv="Content-Security-Policy" content="default-src \'none\'; style-src \'self\'; img-src \'self\' data:; base-uri \'none\'; form-action \'none\'">' +
    '<link rel="canonical" href="' + escapeHtml(canonical) + '">' +
    '<link rel="stylesheet" href="' + escapeHtml(css) + '">' +
    '<title>' + escapeHtml(title) + ' · OutcomeHub</title></head><body>' +
    '<a class="skip" href="#content">' + escapeHtml(tr.skip) + '</a>' +
    '<header class="topbar"><div class="topbar-inner"><a class="brand" href="' +
    escapeHtml(home) + '"><span class="brand-mark">O</span> OutcomeHub</a>' +
    '<nav aria-label="' + escapeHtml(tr.navigation) + '">' + nav + '</nav>' +
    '<a class="language" href="' + escapeHtml(alternateHref) + '" lang="' +
    (lang === 'en' ? 'zh-CN' : 'en') + '">' + escapeHtml(tr.switchLanguage) + '</a></div></header>' +
    '<div class="layout">' + sidebar + '<main id="content">' + content + '</main></div>' +
    '<footer><span>OutcomeHub · v0.1.1</span><span>' + escapeHtml(tr.footer) + '</span></footer>' +
    '</body></html>\n';
}
function renderDocument(input) {
  const lang = languageFor(input);
  const tr = translations[lang];
  const document = fs.readFileSync(input, 'utf8');
  const firstHeading = marked.lexer(document).find((token) => token.type === 'heading');
  const title = firstHeading ? plainHeading(firstHeading.text) : path.posix.basename(input);
  const usedSlugs = new Map();
  marked.use({ renderer: {
    heading(token) {
      const initial = slug(token.text);
      const count = usedSlugs.get(initial) || 0;
      usedSlugs.set(initial, count + 1);
      const id = count ? initial + '-' + (count + 1) : initial;
      return '<h' + token.depth + ' id="' + escapeHtml(id) + '">' +
        this.parser.parseInline(token.tokens) + '<a class="anchor" href="#' +
        encodeURIComponent(id) + '" aria-label="' + escapeHtml(tr.headingLink) +
        '">#</a></h' + token.depth + '>';
    },
    link(token) {
      const href = localLink(input, token.href);
      const titleAttr = token.title ? ' title="' + escapeHtml(token.title) + '"' : '';
      return '<a href="' + escapeHtml(href) + '"' + titleAttr + '>' +
        this.parser.parseInline(token.tokens) + '</a>';
    },
    html(token) { return escapeHtml(token.text); },
  } });
  const body = marked.parse(document, { gfm: true, breaks: false });
  const output = input.slice(0, -3) + '.html';
  const breadcrumb = '<div class="breadcrumbs"><a href="' +
    escapeHtml(relativeTo(output, 'index.html')) + '">' + escapeHtml(tr.home) +
    '</a><span>/</span>' + escapeHtml(title) + '</div>';
  const file = path.join(destination, output);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, pageShell(output, lang, title,
    '<article class="document">' + breadcrumb + body + '</article>', tr.description));
}
for (const input of markdown) renderDocument(input);
for (const input of sources.filter((item) => !item.endsWith('.md'))) {
  const file = path.join(destination, input);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.copyFileSync(input, file);
}
function renderHome(lang, output) {
  const tr = translations[lang];
  const cards = tr.cards.map((card) => '<a class="card" href="' + escapeHtml(card.href) +
    '"><span class="card-number">' + escapeHtml(card.number) + '</span><h2>' +
    escapeHtml(card.title) + '</h2><p>' + escapeHtml(card.description) +
    '</p><span class="card-arrow">↗</span></a>').join('');
  const homepage = '<section class="hero"><div class="hero-copy"><span class="eyebrow">' +
    escapeHtml(tr.version) + '</span><h1>' + escapeHtml(tr.hero) + '</h1><p>' +
    escapeHtml(tr.subtitle) + '</p><div class="hero-actions"><a class="button primary" href="' +
    escapeHtml(tr.cards[0].href) + '">' + escapeHtml(tr.quickStart) +
    '</a><a class="button secondary" href="' + escapeHtml(lang === 'en' ? 'index.zh-CN.html' : 'index.html') +
    '">' + escapeHtml(tr.switchLanguage) +
    '</a></div></div><div class="hero-art" aria-hidden="true"><span>O</span>' +
    '<small>OUTCOME / EVIDENCE / LINEAGE</small></div></section><section class="overview"><div class="section-head"><span class="eyebrow">' +
    escapeHtml(tr.explore) + '</span><h2>' + escapeHtml(tr.findPath) + '</h2></div><div class="cards">' +
    cards + '</div></section><p class="site-note">' + escapeHtml(tr.staticNote) + '</p>';
  fs.writeFileSync(path.join(destination, output),
    pageShell(output, lang, tr.hero, homepage, tr.description));
}
renderHome('en', 'index.html');
renderHome('zh-CN', 'index.zh-CN.html');
const tr = translations.en;
const missing = '<article class="document empty"><h1>' + escapeHtml(tr.notFound) +
  '</h1><p>' + escapeHtml(tr.notFoundBody) +
  '</p><a class="button primary" href="index.html">' + escapeHtml(tr.home) + '</a></article>';
fs.writeFileSync(path.join(destination, '404.html'),
  pageShell('404.html', 'en', tr.notFound, missing, tr.description));
