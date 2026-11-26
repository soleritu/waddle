// Document readers and Word export, from Erlaf (the same author's own project), kept whole:
// PDF, DOCX, XLSX, PPTX, ODT, ODS, ODP, EML, MSG, and RTF become text on this PC, in the page,
// with every structure bounded since the files are untrusted. Nothing is uploaded.
const DOCS = (() => {
let language = "en";
const EN = {"errPageAccess":"Erlaf may not read this page. Open Erlaf on the page with its icon, or right-click the page and choose Erlaf → Diese Seite übernehmen. Browser pages such as chrome:// can never be read.","errPageEmpty":"No text was found on this page.","errAgentExists":"The file already exists: {path}. The agent only creates new Word files.","errUnknown":"Unknown error.","errInvalidBase64":"The file contains invalid data.","errCryptoUnavailable":"Encryption is not available in this browser.","errSealedInvalid":"The encrypted file is damaged or invalid.","errWrongPassphrase":"The passphrase is wrong, or the data was changed.","errNoDecompression":"This browser cannot unpack Office and PDF files. Use a current Microsoft Edge.","errExpandLimit":"The file was not read because it exceeds the safety limit when unpacked.","errCorruptCompression":"The compressed data is damaged.","errOfficeProtected":"The file is protected by a password or sensitivity label and cannot be read.","errNotOffice":"This is not a valid Office file.","errZipUnsupported":"This Office file format is not supported.","errZipCorrupt":"The Office file is damaged.","errXmlDtd":"The file contains disallowed XML definitions and was rejected for security reasons.","errXmlInvalid":"The file contains invalid XML.","errDocxNoMain":"The main content of the Word document is missing.","errXlsxNoSheets":"No readable worksheets were found.","errPptxNoSlides":"No slides were found.","errPdfHeader":"This is not a valid PDF file.","errPdfNoObjects":"The PDF structure could not be read.","errPdfEncrypted":"Protected PDF files are not supported. Remove the protection through an approved process.","errPdfScanned":"The PDF contains only images in a format the browser cannot read (such as CCITT or JBIG2). Save the scan as a JPEG PDF or an image.","xScannedPages":"[Scanned pages without a text layer: {pages}. They are attached as images, in this order.]","errPdfNoText":"No readable text was found in the PDF.","errPdfGarbled":"The PDF text cannot be read reliably, so it is not used.","errBinaryFile":"The file is not a text file.","errNotImage":"The file is not a supported image (PNG, JPEG, WebP, GIF).","errEmptyFile":"The file is empty.","errNoText":"No text was found in the file.","errLegacyOffice":"Legacy Office formats are not supported. Save the file in Office as {target}.","errFileType":"Unsupported file type: {name}","errFileCount":"At most {max} attachments per message.","errFileSize":"{name} exceeds the limit of {max}.","errFilesTotal":"The attachments exceed {max} combined.","errFileDuplicate":"Already attached: {name}","errFileRead":"The file could not be read.","errImagesTotal":"The images in this message exceed {max} combined.","errWaitForFiles":"One moment, the files are still being read.","errFileFailed":"Remove the failed file first: {name}","errNoSheet":"Select at least one worksheet in {name}.","errUrlInvalid":"Enter a complete API address starting with https://.","errUrlScheme":"Only HTTP and HTTPS addresses are supported.","errUrlCredentials":"The API address must not contain credentials.","errUrlQuery":"The API address must not contain parameters or fragments.","errHttpBlocked":"Unencrypted HTTP to other machines is blocked. Use HTTPS or allow it explicitly.","errEndpointPinned":"This file is fixed to {origin}.","errEndpointLocked":"This session is fixed to {origin}. Reload the page to change the address.","errKeyMissing":"Enter the API key.","errKeyCharacters":"The API key contains characters that cannot be transmitted.","errCors":"The server is reachable but does not allow requests from this file (CORS). The API owner must allow the origin “null”. This cannot be changed from here.","errUnreachable":"The server cannot be reached. Check the address, network and certificate.","errHostPermission":"Chrome did not allow access to this API address. Click “Test connection” in Settings and confirm the prompt.","errTimeout":"Timed out: the server did not respond in time.","errConnectionLost":"The connection to the server was lost.","errHttp":"The server replied with error {status}. {detail}","errHttp401":"The API key was rejected (401).","errHttp403":"Access denied (403). {detail}","errHttp404":"Address or model not found (404). {detail}","errHttp413":"The request is too large for the server (413).","errHttp429":"Too many requests (429). Please try again shortly.","errContextExceeded":"The request is too large for the model. Start a new chat or send less text. {detail}","errModelsShape":"The model list has an unexpected format. Enter a model ID at the top.","errStream":"The server reported an error: {detail}","errStreamTooLarge":"The response exceeds the size limit.","errEmptyResponse":"The server returned no data.","errBadResponse":"The server response could not be read.","errTemperature":"The temperature must be between 0 and 2.","errMaxTokens":"The maximum answer length must be a whole number from 1 to 1,000,000.","errContextTokens":"The context window must be a whole number from 1,024 to 10,000,000.","errTimeoutValue":"The timeout must be between 10 and 3,600 seconds.","errChatFull":"This chat has reached the limit of {max} messages. Start a new chat.","errProjectLimit":"At most {max} projects.","errPromptLimit":"At most {max} templates per project.","errChatsLimit":"The limit for saved chats is reached. Remove old chats.","errExportSize":"The export exceeds {max}. Remove chats with large images.","errImportSize":"The file exceeds the limit of {max}.","errImportJson":"The file is not valid JSON.","errImportFormat":"This file format or version is not supported.","errImportContent":"The file contains invalid content ({detail}).","errImportLimits":"The file exceeds the permitted amounts.","errMergeLimit":"Together that would be more than {max} projects. Choose “Replace” or delete projects.","errConfig":"This file’s configuration is invalid and was ignored.","errCspBlocked":"The security policy blocked access to {target}.","xOutlook":"Outlook email","xEmail":"Email","xOdfSlides":"OpenDocument Presentation","xOdfSheet":"OpenDocument Spreadsheet","xOdfText":"OpenDocument Text","xSubject":"Subject","xDate":"Date","xTo":"To","xFrom":"From","xFormFields":"FORM FIELDS","xPage":"PAGE","xNotes":"Speaker notes","xHiddenMark":"hidden","xSlide":"SLIDE","xFalse":"FALSE","xTrue":"TRUE","xCutCols":"[CUT: only the first {limit} columns were read.]","xCutRows":"[CUT: only the first {limit} of {total} rows were read.]","xRows":"[ROWS]","xNoStats":"[no column statistics]","xOverview":"[LOCAL OVERVIEW; first filled row assumed to be the header; formulas not recalculated]","xSheetHead":"{rows} row(s) | {cols} column(s)","xFormula":"Formula","xNoFormulaValue":"[Formula without a stored value]","xRow":"Row","xColumn":"Column","xEmpty":"empty","xSheet":"WORKSHEET","xDistinct":"{count} distinct values; examples: {samples}","xDates":"dates: from {from} to {to}","xNumeric":"numeric: min {min}, max {max}, mean {mean}, sum {sum}","xValues":"{count} value(s), {blank} blank","xDeleted":"deleted","xComments":"Comments","xEndnotes":"Endnotes","xFootnotes":"Footnotes","xFooter":"Footer","xHeader":"Header","xTable":"Table","errNotChatModel":"{model} is not a chat model. Choose another one.","errNoVision":"{model} cannot read images. Set an image model in Settings.","errAgentChanged":"The file {path} has changed in the meantime and was not overwritten.","errAgentCutOff":"The model’s reply was cut off at the token limit. Nothing was written.","errCorsLocal":"The local model server is running but refuses requests from this file. LM Studio: switch on “Enable CORS”. Ollama: set OLLAMA_ORIGINS to * and restart. vLLM: set --allowed-origins.","errAgentNoFolder":"Choose a folder for the agent first.","errAgentUnsupported":"This browser does not allow folder access.","errAgentBlocked":"Folder access was blocked by the browser or a policy.","errAgentPath":"Invalid path: {path}","errAgentArgs":"Missing input for {tool}.","errAgentFileType":"This file type cannot be read: {path}","errAgentWriteType":"This file must not be written: {path}","errAgentEncoding":"The file {path} is not UTF-8 ({encoding}) and will not be changed.","errAgentNotFound":"File not found: {path}","errAgentMatch":"The text must occur exactly once in {path}; found: {count}.","errAgentPermission":"Write access to the folder was not granted.","errCompactBlocked":"The summary looked suspicious and was discarded. The chat is unchanged.","errFolderLocked":"This chat is encrypted.","errFolderSave":"The chat could not be saved to the folder.","errStreamCut":"The provider stopped the answer partway. Free models often do this on long answers. Try again or choose another model.","errCompactLength":"The summary was too long and was discarded. The chat is unchanged.","errCompactEmpty":"The model returned no summary. The chat is unchanged.","errCompactNoGain":"The summary would not be shorter than the messages. The chat is unchanged."};
function tr(key, params){ let s = EN[key] || key; for (const [k, v] of Object.entries(params || {})) s = s.replaceAll("{" + k + "}", v); return s; }
function toolButton(){ return document.createElement("span"); }
function copyToClipboard(text){ navigator.clipboard?.writeText(text); }
// ---- from Erlaf: 00-config.js
// Product constants. Limits here are shared by creation and import paths, so that
// anything the app lets a user build can always be exported and imported again.
const APP_VERSION = '1.0.0';
const APP_RELEASE_DATE = '2026-11-12';
const UPDATE_REMINDER_DAYS = 183;

const LIMITS = Object.freeze({
  imageBytes: 8 * 1024 * 1024,
  imagesTotalBytes: 32 * 1024 * 1024,
  textBytes: 2 * 1024 * 1024,
  officeBytes: 25 * 1024 * 1024,
  pdfBytes: 40 * 1024 * 1024,
  totalFileBytes: 100 * 1024 * 1024,
  filesPerMessage: 30,
  extractedPerFile: 350000,
  extractedPerMessage: 1200000,
  importBytes: 150 * 1024 * 1024,
  sealedImportBytes: 210 * 1024 * 1024,
  zipEntries: 20000,
  zipEntryBytes: 64 * 1024 * 1024,
  zipTotalBytes: 160 * 1024 * 1024,
  pdfStreamBytes: 64 * 1024 * 1024,
  pdfTotalBytes: 200 * 1024 * 1024,
  pdfPages: 5000,
  pdfDepth: 48,
  scanPages: 20,
  excelRows: 2500,
  excelCols: 1000,
  pptSlides: 300,
  projects: 30,
  chatsPerProject: 500,
  chatsTotal: 1000,
  messagesPerChat: 500,
  messagesTotal: 10000,
  promptsPerProject: 100,
  messageChars: 10000000,
  responseChars: 5000000,
  projectName: 80,
  chatTitle: 200,
  promptName: 80,
  promptText: 12000,
  instructions: 20000,
  systemPrompt: 100000,
  modelId: 300,
  models: 2000,
  errorDetail: 500,
  sseBuffer: 8 * 1024 * 1024,
  passphraseMin: 12,
});

const DEFAULTS = Object.freeze({
  temperature: '0.7',
  maxTokens: '16384',
  contextTokens: '32768',
  timeoutSeconds: '300',
  lockMinutes: '15',
});

// Rough size estimate for German text. Deliberately pessimistic: warning early is
// harmless, an over-long request fails.
const CHARS_PER_TOKEN = 3;
const TOKENS_PER_IMAGE = 1100;
const DOC_BUDGET_SHARE = 0.6;

const PBKDF2_ITERATIONS = 600000;
const PBKDF2_MIN_ITERATIONS = 100000;
const PBKDF2_MAX_ITERATIONS = 10000000;

const TEXT_EXTENSIONS = new Set([
  'txt', 'md', 'csv', 'tsv', 'json', 'xml', 'log', 'yaml', 'yml', 'html', 'htm', 'css', 'js', 'ts', 'py',
  'sql', 'java', 'cs', 'c', 'cpp', 'h', 'sh', 'ps1', 'bat', 'toml', 'ini', 'conf', 'properties',
  'ipynb', 'tex', 'rst', 'srt', 'vtt',
]);
const OFFICE_EXTENSIONS = new Set(['docx', 'xlsx', 'xlsm', 'pptx', 'odt', 'ods', 'odp']);
const MAIL_EXTENSIONS = new Set(['eml', 'msg', 'rtf']);
const LEGACY_OFFICE = Object.freeze({ doc: 'DOCX', xls: 'XLSX', ppt: 'PPTX' });
const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'webp', 'gif']);

const STOPWORDS = new Set([
  'the', 'and', 'for', 'with', 'that', 'this', 'from', 'please', 'about', 'document', 'file',
  'summarize', 'summary', 'explain', 'what', 'which', 'into', 'have', 'does', 'can',
  'der', 'die', 'das', 'den', 'dem', 'des', 'ein', 'eine', 'einen', 'einem', 'einer', 'und',
  'oder', 'aber', 'mit', 'von', 'zu', 'zur', 'zum', 'auf', 'aus', 'bei', 'nach', 'wie', 'was',
  'wer', 'wann', 'warum', 'ist', 'sind', 'war', 'wird', 'werden', 'kann', 'bitte', 'sie',
  'dokument', 'datei', 'zusammenfassung', 'fasse', 'fassen', 'zusammen', 'erkläre', 'nicht',
]);

const BUILTIN_PROMPTS = Object.freeze([
  {
    id: 'summary', de: 'Dokument zusammenfassen', en: 'Summarize a document',
    deText: 'Fasse die bereitgestellten Unterlagen strukturiert zusammen. Nenne Zweck, ' +
      'Kernaussagen, wichtige Daten, offene Punkte und mögliche nächste Schritte. Trenne ' +
      'belegte Informationen von Annahmen.',
    enText: 'Summarize the provided materials in a structured way. Cover purpose, key points, ' +
      'important dates, open issues and possible next steps. Separate supported facts from assumptions.',
  },
  {
    id: 'proofread', de: 'Korrekturlesen', en: 'Proofread and improve',
    deText: 'Prüfe den folgenden Text auf Rechtschreibung, Grammatik, Zeichensetzung, ' +
      'Verständlichkeit und sachlichen Stil. Erhalte die Bedeutung. Gib zuerst eine ' +
      'überarbeitete Fassung und danach eine kurze Liste wesentlicher Änderungen aus.',
    enText: 'Review the following text for spelling, grammar, punctuation, clarity and professional ' +
      'tone. Preserve its meaning. Return a revised version first, followed by a short list of ' +
      'material changes.',
  },
  {
    id: 'official-letter', de: 'Behördliches Schreiben entwerfen', en: 'Draft a formal letter',
    deText: 'Erstelle einen sachlichen, höflichen und klar strukturierten Entwurf für ein ' +
      'behördliches Schreiben. Verwende nur Angaben aus dem bereitgestellten Material. ' +
      'Kennzeichne fehlende Angaben als [zu ergänzen] und erfinde keine Rechtsgrundlagen ' +
      'oder Tatsachen.',
    enText: 'Draft a factual, courteous and clearly structured administrative letter. Use only ' +
      'information in the supplied material. Mark missing details as [to be completed] and do not ' +
      'invent legal bases or facts.',
  },
  {
    id: 'compare', de: 'Dokumente vergleichen', en: 'Compare documents',
    deText: 'Vergleiche die bereitgestellten Dokumente systematisch. Erstelle eine Tabelle ' +
      'mit Abschnitt, bisherigem Inhalt, neuem Inhalt und Bedeutung der Änderung. Unterscheide ' +
      'sprachliche von inhaltlichen Änderungen und weise auf Unsicherheiten hin.',
    enText: 'Compare the supplied documents systematically. Create a table with section, previous ' +
      'content, new content and significance of the change. Distinguish wording changes from ' +
      'substantive changes and flag uncertainty.',
  },
  {
    id: 'meeting', de: 'Sitzung vorbereiten', en: 'Prepare a meeting',
    deText: 'Erstelle aus den Unterlagen eine Sitzungsvorbereitung mit Ziel, Tagesordnung, ' +
      'Entscheidungsbedarf, Hintergrund, offenen Fragen und einer Liste benötigter Unterlagen. ' +
      'Nenne die Quelle zu jedem wesentlichen Punkt.',
    enText: 'Prepare a meeting brief from the materials: objective, agenda, decisions needed, ' +
      'background, open questions and required documents. Identify the source for each material point.',
  },
  {
    id: 'minutes', de: 'Besprechungsprotokoll', en: 'Draft meeting minutes',
    deText: 'Erstelle einen Entwurf für ein Ergebnisprotokoll. Gliedere in Teilnehmende ' +
      '(nur soweit genannt), Tagesordnungspunkte, wesentliche Diskussionsergebnisse, Beschlüsse, ' +
      'Aufgaben, verantwortliche Personen und Fristen. Erfinde keine fehlenden Angaben.',
    enText: 'Draft concise meeting minutes. Structure them into attendees (only if provided), agenda ' +
      'items, key discussion outcomes, decisions, tasks, owners and deadlines. Do not invent missing details.',
  },
  {
    id: 'extract', de: 'Daten strukturiert extrahieren', en: 'Extract structured data',
    deText: 'Extrahiere die relevanten Informationen aus den Unterlagen in eine übersichtliche ' +
      'Tabelle. Verwende eindeutige Spaltenüberschriften. Kennzeichne fehlende, ' +
      'widersprüchliche oder nicht eindeutig zuordenbare Angaben.',
    enText: 'Extract relevant information from the materials into a clear table with unambiguous ' +
      'column headings. Flag missing, conflicting or ambiguous information.',
  },
  {
    id: 'plain-language', de: 'Klare Sprache', en: 'Plain-language rewrite',
    deText: 'Formuliere den folgenden Text in klarer, gut verständlicher deutscher Sprache um. ' +
      'Erhalte den fachlichen Inhalt, erkläre notwendige Fachbegriffe und verwende ' +
      'kurze Sätze. Füge keine neuen Aussagen hinzu.',
    enText: 'Rewrite the following text in clear, accessible English. Preserve the technical meaning, ' +
      'explain necessary terminology and use short sentences. Do not add new claims.',
  },
  {
    id: 'excel', de: 'Excel-Daten analysieren', en: 'Analyze spreadsheet data',
    deText: 'Analysiere die bereitgestellten Tabellendaten. Beschreibe Datenstruktur und ' +
      'Auffälligkeiten, prüfe fehlende oder ungewöhnliche Werte, nenne nachvollziehbare ' +
      'Kennzahlen und schlage geeignete Auswertungen vor. Weise auf Datenqualitätsprobleme hin.',
    enText: 'Analyze the supplied tabular data. Describe its structure and anomalies, inspect missing ' +
      'or unusual values, provide transparent metrics and suggest useful analyses. Identify data-quality issues.',
  },
  {
    id: 'action-items', de: 'Aufgaben und Fristen erkennen', en: 'Extract actions and deadlines',
    deText: 'Finde alle Aufgaben, Zuständigkeiten, Abhängigkeiten und Fristen in den ' +
      'bereitgestellten Unterlagen. Gib sie als Tabelle aus. Wenn eine Zuständigkeit oder Frist ' +
      'nicht genannt ist, schreib „nicht angegeben“.',
    enText: 'Identify all tasks, owners, dependencies and deadlines in the supplied materials. Return ' +
      'a table. Where an owner or deadline is not specified, write “not specified”.',
  },
]);

// ---- from Erlaf: 20-util.js
// Small shared helpers. All user-facing failures use KbError so they can be translated.
const $ = (id) => document.getElementById(id);

class KbError extends Error {
  constructor(key, params) {
    super(key);
    this.name = 'KbError';
    this.key = key;
    this.params = params || {};
  }
}

function el(tag, props, ...children) {
  const node = document.createElement(tag);
  if (props) {
    for (const [name, value] of Object.entries(props)) {
      if (value === undefined || value === null || value === false) continue;
      if (name === 'class') node.className = value;
      else if (name === 'text') node.textContent = value;
      else if (name === 'on') for (const [ev, fn] of Object.entries(value)) node.addEventListener(ev, fn);
      else if (name in node && name !== 'list') node[name] = value;
      else node.setAttribute(name, value === true ? '' : value);
    }
  }
  for (const child of children) if (child !== null && child !== undefined) node.append(child);
  return node;
}

const SVG_NS = 'http://www.w3.org/2000/svg';

function svgUse(id, className) {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('class', className);
  svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS(SVG_NS, 'use');
  use.setAttribute('href', `#${id}`);
  svg.append(use);
  return svg;
}
const icon = (name) => svgUse(`i-${name}`, 'icon');
const logoMark = (size) => svgUse('kb-logo', `logo ${size}`);

function toolButton(name, label, onClick) {
  return el('button', { type: 'button', class: 'ghost', on: { click: onClick } }, icon(name), el('span', { text: label }));
}

function randomHex(bytes) {
  const data = crypto.getRandomValues(new Uint8Array(bytes));
  return Array.from(data, (b) => b.toString(16).padStart(2, '0')).join('');
}
const makeId = () => `kb-${randomHex(12)}`;

function formatBytes(n) {
  if (n < 1024) return `${n} B`;
  const locale = language === 'de' ? 'de-DE' : 'en-GB';
  const fmt = (v) => v.toLocaleString(locale, { maximumFractionDigits: 1 });
  return n < 1048576 ? `${fmt(n / 1024)} KB` : `${fmt(n / 1048576)} MB`;
}
const formatNumber = (n) => n.toLocaleString(language === 'de' ? 'de-DE' : 'en-GB');

function fileExtension(name) {
  const i = name.lastIndexOf('.');
  return i < 0 ? '' : name.slice(i + 1).toLowerCase();
}

// Names reach the model and the UI. Control and bidi characters could disguise a file
// (for example "exe.txt" written right-to-left) or inject line breaks into the prompt.
function safeName(name) {
  return String(name).replace(/[\u0000-\u001f\u007f-\u009f\u200e\u200f\u202a-\u202e\u2066-\u2069]/g, '').slice(0, 255);
}

// Removes characters that are invisible or meaningless in text sent to a model, including
// Unicode tag characters, which can hide instructions from a human reader.
function cleanText(text) {
  return text
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f\u00ad]/g, '')
    .replace(/\udb40[\udc00-\udc7f]/g, '')
    .replace(/[\ufdd0-\ufdef\ufffe\uffff]/g, '');
}

const yieldToUi = () => new Promise((resolve) => setTimeout(resolve, 0));

function bytesToBase64(bytes) {
  const step = 0x8000 - (0x8000 % 3);
  const parts = [];
  for (let i = 0; i < bytes.length; i += step) {
    parts.push(btoa(String.fromCharCode.apply(null, bytes.subarray(i, i + step))));
  }
  return parts.join('');
}

function base64ToBytes(text) {
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(text) || text.length % 4) throw new KbError('errInvalidBase64');
  const pad = text.endsWith('==') ? 2 : text.endsWith('=') ? 1 : 0;
  const out = new Uint8Array((text.length / 4) * 3 - pad);
  const step = 0x8000;
  let pos = 0;
  for (let i = 0; i < text.length; i += step) {
    const bin = atob(text.slice(i, i + step));
    for (let j = 0; j < bin.length; j++) out[pos++] = bin.charCodeAt(j);
  }
  return out;
}

function downloadFile(name, data, type) {
  const url = URL.createObjectURL(new Blob([data], { type }));
  const a = el('a', { href: url, download: name, hidden: true });
  document.body.append(a);
  a.click();
  a.remove();
  // Large files can take a while to hand over to the download manager.
  setTimeout(() => URL.revokeObjectURL(url), 120000);
}

const stamp = () => new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');

// ---- from Erlaf: 40-zip.js
// Minimal ZIP reader for Office files. Every size comes from an untrusted file, so
// declared values are checked against reality and actual output is budgeted across
// the whole archive, not only per entry.
const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(data) {
  let crc = 0xffffffff;
  for (let i = 0; i < data.length; i++) crc = CRC_TABLE[(crc ^ data[i]) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function normalizeZipPath(path) {
  const parts = [];
  for (const p of path.replace(/\\/g, '/').split('/')) {
    if (!p || p === '.') continue;
    if (p === '..') parts.pop();
    else parts.push(p);
  }
  return parts.join('/');
}

function concatChunks(chunks, size) {
  const out = new Uint8Array(size);
  let offset = 0;
  for (const c of chunks) {
    out.set(c, offset);
    offset += c.length;
  }
  return out;
}

// Inflates with a hard output cap. With `tolerant`, data decoded before an error is
// returned: PDF writers often leave stray bytes after the end of a zlib stream.
async function inflate(bytes, format, limit, tolerant) {
  if (typeof DecompressionStream === 'undefined') throw new KbError('errNoDecompression');
  const reader = new Blob([bytes]).stream().pipeThrough(new DecompressionStream(format)).getReader();
  const chunks = [];
  let size = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.length;
      if (size > limit) {
        reader.cancel().catch(() => {});
        throw new KbError('errExpandLimit');
      }
      chunks.push(value);
    }
  } catch (e) {
    if (e instanceof KbError) throw e;
    if (!tolerant || !size) throw new KbError('errCorruptCompression');
  }
  return concatChunks(chunks, size);
}

function decodeXmlBytes(bytes) {
  if (bytes[0] === 0xff && bytes[1] === 0xfe) return new TextDecoder('utf-16le').decode(bytes);
  if (bytes[0] === 0xfe && bytes[1] === 0xff) return new TextDecoder('utf-16be').decode(bytes);
  return new TextDecoder('utf-8').decode(bytes);
}

async function readZip(file) {
  const buffer = await file.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  if (bytes.length >= 8 && bytes[0] === 0xd0 && bytes[1] === 0xcf && bytes[2] === 0x11 && bytes[3] === 0xe0) {
    // Password-protected and sensitivity-labelled Office files are OLE containers.
    throw new KbError('errOfficeProtected');
  }
  const view = new DataView(buffer);
  const min = Math.max(0, bytes.length - 65557);
  let eocd = -1;
  for (let i = bytes.length - 22; i >= min; i--) {
    if (view.getUint32(i, true) === 0x06054b50) { eocd = i; break; }
  }
  if (eocd < 0) throw new KbError('errNotOffice');
  const total = view.getUint16(eocd + 10, true);
  const cdSize = view.getUint32(eocd + 12, true);
  const cdOffset = view.getUint32(eocd + 16, true);
  if (view.getUint16(eocd + 4, true) !== 0 || view.getUint16(eocd + 6, true) !== 0 ||
      view.getUint16(eocd + 8, true) !== total) throw new KbError('errZipUnsupported');
  if (total === 0xffff || cdOffset === 0xffffffff || cdSize === 0xffffffff) throw new KbError('errZipUnsupported');
  if (total > LIMITS.zipEntries || cdOffset + cdSize > bytes.length) throw new KbError('errZipCorrupt');

  const entries = new Map();
  const decoder = new TextDecoder('utf-8');
  let ptr = cdOffset;
  for (let n = 0; n < total; n++) {
    if (ptr + 46 > bytes.length || view.getUint32(ptr, true) !== 0x02014b50) throw new KbError('errZipCorrupt');
    const flags = view.getUint16(ptr + 8, true);
    const method = view.getUint16(ptr + 10, true);
    const crc = view.getUint32(ptr + 16, true);
    const compressed = view.getUint32(ptr + 20, true);
    const uncompressed = view.getUint32(ptr + 24, true);
    const nameLen = view.getUint16(ptr + 28, true);
    const extraLen = view.getUint16(ptr + 30, true);
    const commentLen = view.getUint16(ptr + 32, true);
    const localOffset = view.getUint32(ptr + 42, true);
    if (flags & 1) throw new KbError('errOfficeProtected');
    if (compressed === 0xffffffff || uncompressed === 0xffffffff || localOffset === 0xffffffff) {
      throw new KbError('errZipUnsupported');
    }
    const name = normalizeZipPath(decoder.decode(bytes.subarray(ptr + 46, ptr + 46 + nameLen)));
    if (name && !name.endsWith('/')) entries.set(name, { method, crc, compressed, uncompressed, localOffset });
    ptr += 46 + nameLen + extraLen + commentLen;
  }

  // Part names are case-insensitive in OPC, and some producers differ from the convention.
  const byLowerName = new Map();
  for (const name of entries.keys()) byLowerName.set(name.toLowerCase(), name);
  const resolve = (name) => byLowerName.get(normalizeZipPath(name).toLowerCase());

  let expanded = 0;
  async function readBytes(name) {
    const realName = resolve(name);
    if (realName === undefined) return null;
    const ent = entries.get(realName);
    const o = ent.localOffset;
    if (o + 30 > bytes.length || view.getUint32(o, true) !== 0x04034b50) throw new KbError('errZipCorrupt');
    const begin = o + 30 + view.getUint16(o + 26, true) + view.getUint16(o + 28, true);
    const end = begin + ent.compressed;
    if (end > bytes.length) throw new KbError('errZipCorrupt');
    const remaining = LIMITS.zipTotalBytes - expanded;
    const limit = Math.min(LIMITS.zipEntryBytes, remaining);
    if (ent.uncompressed > limit) throw new KbError('errExpandLimit');
    let out;
    if (ent.method === 0) out = bytes.subarray(begin, end);
    else if (ent.method === 8) out = await inflate(bytes.subarray(begin, end), 'deflate-raw', limit, false);
    else throw new KbError('errZipUnsupported');
    expanded += out.length;
    if (out.length !== ent.uncompressed || crc32(out) !== ent.crc) throw new KbError('errZipCorrupt');
    return out;
  }
  async function readText(name) {
    const data = await readBytes(name);
    return data === null ? null : decodeXmlBytes(data);
  }
  return { names: () => Array.from(entries.keys()), has: (n) => resolve(n) !== undefined, readText };
}

// Office XML never legitimately has a DTD. Refusing it outright removes entity
// expansion and external-entity behaviour from consideration.
function parseXml(text) {
  if (text.includes('<!DOCTYPE') || text.includes('<!ENTITY')) throw new KbError('errXmlDtd');
  const doc = new DOMParser().parseFromString(text, 'application/xml');
  if (doc.getElementsByTagName('parsererror').length) throw new KbError('errXmlInvalid');
  return doc;
}

const allByName = (root, localName) => Array.from(root.getElementsByTagNameNS('*', localName));

function relationshipId(node) {
  for (const attr of Array.from(node.attributes)) {
    if (attr.localName === 'id' && attr.prefix) return attr.value;
  }
  return node.getAttribute('r:id');
}

async function readRelationships(zip, path) {
  const map = new Map();
  const text = await zip.readText(path);
  if (!text) return map;
  for (const r of allByName(parseXml(text), 'Relationship')) {
    map.set(r.getAttribute('Id'), { type: r.getAttribute('Type') || '', target: r.getAttribute('Target') || '' });
  }
  return map;
}

function resolveTarget(baseDir, target) {
  return target.startsWith('/') ? normalizeZipPath(target) : normalizeZipPath(`${baseDir}/${target}`);
}

// Finds the main part through the package relationships rather than a fixed path.
async function findMainPart(zip, fallback) {
  const rels = await readRelationships(zip, '_rels/.rels');
  for (const rel of rels.values()) {
    if (/\/officeDocument$/.test(rel.type)) {
      const path = resolveTarget('', rel.target);
      if (zip.has(path)) return path;
    }
  }
  return fallback;
}

const dirOf = (path) => path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
const relsPathOf = (path) => `${dirOf(path)}/_rels/${path.slice(path.lastIndexOf('/') + 1)}.rels`;

// ---- from Erlaf: 41-office.js
// Word, Excel and PowerPoint text extraction. Each extractor returns text plus a report
// of what it left out, so the user can see it: an omission must never be silent.

// ---------------------------------------------------------------- Word
function wordRunIsHidden(run) {
  const props = Array.from(run.children).find((c) => c.localName === 'rPr');
  if (!props) return false;
  const vanish = Array.from(props.children).find((c) => c.localName === 'vanish');
  if (!vanish) return false;
  const val = vanish.getAttributeNS(vanish.namespaceURI, 'val') ?? vanish.getAttribute('w:val');
  return val !== '0' && val !== 'false';
}

function wordInlineText(paragraph, ctx) {
  let out = '';
  const walk = (node) => {
    for (const child of Array.from(node.children)) {
      const name = child.localName;
      if (name === 't') out += child.textContent;
      else if (name === 'tab') out += '\t';
      else if (name === 'br' || name === 'cr') out += '\n';
      else if (name === 'noBreakHyphen') out += '-';
      else if (name === 'rPr' || name === 'pPr' || name === 'instrText') continue;
      else if (name === 'Fallback') continue; // Choice already carries this content.
      else if (name === 'del' || name === 'moveFrom') {
        const text = allByName(child, 'delText').concat(allByName(child, 't')).map((n) => n.textContent).join('');
        if (text) {
          ctx.report.trackedDeletions++;
          if (ctx.includeHidden) out += `[[${tr('xDeleted')}: ${text}]]`;
        }
      } else if (name === 'txbxContent') {
        const inner = wordBlocks(child, ctx).join('\n');
        if (inner) out += `${out ? '\n' : ''}${inner}\n`;
      } else if (name === 'r' && wordRunIsHidden(child)) {
        ctx.report.hiddenText++;
        if (ctx.includeHidden) walk(child);
      } else walk(child);
    }
  };
  walk(paragraph);
  return out.trimEnd();
}

function wordTable(table, ctx) {
  const rows = [];
  for (const row of Array.from(table.children)) {
    if (row.localName !== 'tr') continue;
    const cells = [];
    for (const cell of Array.from(row.children)) {
      if (cell.localName === 'tc') cells.push(wordBlocks(cell, ctx).join(' / ').replace(/\n/g, ' '));
    }
    rows.push(cells.join(' | '));
  }
  return rows.length ? `[${tr('xTable')}]\n${rows.join('\n')}\n[/${tr('xTable')}]` : '';
}

function wordBlocks(container, ctx) {
  const blocks = [];
  for (const child of Array.from(container.children)) {
    const name = child.localName;
    if (name === 'p') {
      const text = wordInlineText(child, ctx);
      if (text) blocks.push(text);
    } else if (name === 'tbl') {
      const text = wordTable(child, ctx);
      if (text) blocks.push(text);
    } else if (['sdt', 'sdtContent', 'customXml', 'smartTag', 'ins', 'moveTo', 'Choice', 'AlternateContent'].includes(name)) {
      blocks.push(...wordBlocks(child, ctx));
    }
  }
  return blocks;
}

async function extractDocx(file, options) {
  const zip = await readZip(file);
  const mainPath = await findMainPart(zip, 'word/document.xml');
  const main = await zip.readText(mainPath);
  if (!main) throw new KbError('errDocxNoMain');
  const doc = parseXml(main);
  const body = allByName(doc, 'body')[0];
  if (!body) throw new KbError('errDocxNoMain');
  const report = { trackedDeletions: 0, hiddenText: 0, comments: 0 };
  const ctx = { report, includeHidden: options.includeHidden };
  const sections = wordBlocks(body, ctx);

  const dir = dirOf(mainPath);
  const partsIn = (re) => zip.names().filter((n) => n.startsWith(`${dir}/`) && re.test(n.slice(dir.length + 1)));
  for (const name of partsIn(/^(header|footer)\d*\.xml$/i)) {
    const text = await zip.readText(name);
    const part = parseXml(text).documentElement;
    const blocks = wordBlocks(part, ctx);
    if (blocks.length) sections.push(`[${tr(/header/i.test(name) ? 'xHeader' : 'xFooter')}]\n${blocks.join('\n')}`);
  }
  for (const [part, label, item] of [['footnotes.xml', tr('xFootnotes'), 'footnote'], ['endnotes.xml', tr('xEndnotes'), 'endnote']]) {
    const text = await zip.readText(`${dir}/${part}`);
    if (!text) continue;
    const notes = allByName(parseXml(text), item)
      .filter((n) => !/separator|continuation/i.test(n.getAttributeNS(n.namespaceURI, 'type') || n.getAttribute('w:type') || ''))
      .map((n) => wordBlocks(n, ctx).join(' '))
      .filter(Boolean);
    if (notes.length) sections.push(`[${label}]\n${notes.join('\n')}`);
  }
  const commentsXml = await zip.readText(`${dir}/comments.xml`);
  if (commentsXml) {
    const comments = allByName(parseXml(commentsXml), 'comment');
    report.comments = comments.length;
    if (options.includeHidden && comments.length) {
      sections.push(`[${tr('xComments')}]\n${comments.map((c) => wordBlocks(c, ctx).join(' ')).join('\n')}`);
    }
  }
  return { text: sections.join('\n\n'), format: 'Word DOCX', report };
}

// ---------------------------------------------------------------- Excel
const BUILTIN_DATE_FORMATS = new Set([14, 15, 16, 17, 18, 19, 20, 21, 22, 27, 30, 36, 45, 46, 47, 50, 57]);

function formatLooksLikeDate(code) {
  const cleaned = code.replace(/\[[^\]]*\]/g, '').replace(/"[^"]*"/g, '').replace(/\\./g, '');
  return /[ymdhs]/i.test(cleaned) && !/^(general|[#0.,%\s;-]+)$/i.test(cleaned);
}

async function readExcelStyles(zip, dir) {
  const dateStyles = new Set();
  const text = await zip.readText(`${dir}/styles.xml`);
  if (!text) return dateStyles;
  const doc = parseXml(text);
  const custom = new Map();
  for (const f of allByName(doc, 'numFmt')) {
    custom.set(Number(f.getAttribute('numFmtId')), f.getAttribute('formatCode') || '');
  }
  const cellXfs = allByName(doc, 'cellXfs')[0];
  if (!cellXfs) return dateStyles;
  Array.from(cellXfs.children).forEach((xf, index) => {
    const id = Number(xf.getAttribute('numFmtId'));
    if (BUILTIN_DATE_FORMATS.has(id) || (custom.has(id) && formatLooksLikeDate(custom.get(id)))) {
      dateStyles.add(index);
    }
  });
  return dateStyles;
}

function excelSerialToText(serial, date1904) {
  const days = Math.floor(serial);
  const seconds = Math.round((serial - days) * 86400);
  const pad = (n) => String(n).padStart(2, '0');
  const time = `${pad(Math.floor(seconds / 3600))}:${pad(Math.floor(seconds / 60) % 60)}`;
  if (days === 0 && !date1904) return time;
  // Base 1899-12-30 absorbs Excel's fictitious 29 February 1900.
  const base = date1904 ? Date.UTC(1904, 0, 1) : Date.UTC(1899, 11, 30);
  const date = new Date(base + days * 86400000).toISOString().slice(0, 10);
  return seconds ? `${date} ${time}` : date;
}

function columnName(n) {
  let s = '';
  while (n > 0) {
    const r = (n - 1) % 26;
    s = String.fromCharCode(65 + r) + s;
    n = Math.floor((n - 1) / 26);
  }
  return s;
}

function columnIndex(ref) {
  const m = /^([A-Z]+)\d*$/i.exec(ref || '');
  if (!m) return 0;
  let col = 0;
  for (const ch of m[1].toUpperCase()) col = col * 26 + ch.charCodeAt(0) - 64;
  return col;
}

function hiddenColumns(sheetDoc) {
  const hidden = new Set();
  for (const col of allByName(sheetDoc, 'col')) {
    if (col.getAttribute('hidden') !== '1' && col.getAttribute('hidden') !== 'true') continue;
    const min = Number(col.getAttribute('min'));
    const max = Math.min(Number(col.getAttribute('max')), LIMITS.excelCols);
    for (let c = min; c <= max; c++) hidden.add(c);
  }
  return hidden;
}

function sharedStringText(si) {
  let out = '';
  for (const child of Array.from(si.children)) {
    if (child.localName === 't') out += child.textContent;
    else if (child.localName === 'r') out += allByName(child, 't').map((t) => t.textContent).join('');
    // rPh holds phonetic hints, not cell text.
  }
  return out;
}

function readCell(c, env) {
  const type = c.getAttribute('t');
  const vNode = allByName(c, 'v')[0];
  const fNode = allByName(c, 'f')[0];
  const raw = vNode ? vNode.textContent : '';
  const cell = { value: raw, numeric: false, date: false, formula: fNode ? fNode.textContent : '' };
  if (type === 's') {
    if (raw !== '') cell.value = env.shared[Number(raw)] ?? '';
  } else if (type === 'inlineStr') {
    const is = allByName(c, 'is')[0];
    cell.value = is ? sharedStringText(is) : '';
  } else if (type === 'b') {
    cell.value = tr(raw === '1' ? 'xTrue' : 'xFalse');
  } else if (!type || type === 'n') {
    const num = Number(raw);
    if (raw !== '' && Number.isFinite(num)) {
      if (env.dateStyles.has(Number(c.getAttribute('s')))) {
        cell.value = excelSerialToText(num, env.date1904);
        cell.date = true;
      } else {
        cell.numeric = true;
        cell.number = num;
      }
    }
  }
  return cell;
}

function columnStats(records, maxCol, headers) {
  const lines = [];
  for (let col = 1; col <= maxCol; col++) {
    const cells = records.slice(1).map((r) => r.cells.get(col)).filter((c) => c && String(c.value).trim() !== '');
    if (!cells.length) continue;
    const blank = records.length - 1 - cells.length;
    const nums = cells.filter((c) => c.numeric).map((c) => c.number);
    let line = `- ${headers[col]} (${columnName(col)}): ${tr('xValues', { count: cells.length, blank })}`;
    if (nums.length >= Math.max(1, Math.ceil(cells.length * 0.75))) {
      let min = Infinity;
      let max = -Infinity;
      let sum = 0;
      for (const n of nums) {
        if (n < min) min = n;
        if (n > max) max = n;
        sum += n;
      }
      line += `; ${tr('xNumeric', { min, max, mean: (sum / nums.length).toFixed(3), sum: sum.toFixed(3) })}`;
    } else if (cells.every((c) => c.date)) {
      const dates = cells.map((c) => c.value).sort();
      line += `; ${tr('xDates', { from: dates[0], to: dates[dates.length - 1] })}`;
    } else {
      const unique = new Set(cells.map((c) => String(c.value)));
      const samples = Array.from(unique).slice(0, 5).map((v) => v.replace(/[\r\n]+/g, ' ').slice(0, 45));
      line += `; ${tr('xDistinct', { count: unique.size, samples: samples.join(' | ') })}`;
    }
    lines.push(line);
  }
  return lines;
}

// Only the first rows are used, so the XML is cut there before parsing. A sheet with
// hundreds of thousands of rows would otherwise freeze the page while building a DOM.
function limitSheetXml(xml) {
  const tag = /<((?:[\w.-]+:)?)row[\s/>]/g;
  let total = 0;
  let cut = -1;
  let prefix = '';
  let match;
  while ((match = tag.exec(xml)) !== null) {
    total++;
    if (total === LIMITS.excelRows + 1) {
      cut = match.index;
      prefix = match[1];
    }
  }
  if (cut < 0) return { xml, total };
  return { xml: `${xml.slice(0, cut)}</${prefix}sheetData></${prefix}worksheet>`, total };
}

function excelSheetText(name, xml, env, options, report) {
  const limited = limitSheetXml(xml);
  const doc = parseXml(limited.xml);
  const rows = allByName(doc, 'row');
  const rowTotal = limited.total;
  const hiddenCols = options.includeHidden ? new Set() : hiddenColumns(doc);
  if (hiddenCols.size) report.hiddenColumns += hiddenCols.size;
  const records = [];
  let maxCol = 0;
  let droppedCols = false;
  const limit = Math.min(rows.length, LIMITS.excelRows);
  for (let i = 0; i < limit; i++) {
    const row = rows[i];
    const isHidden = row.getAttribute('hidden') === '1' || row.getAttribute('hidden') === 'true';
    if (isHidden) {
      report.hiddenRows++;
      if (!options.includeHidden) continue;
    }
    const cells = new Map();
    let running = 0;
    for (const c of Array.from(row.children)) {
      if (c.localName !== 'c') continue;
      const col = columnIndex(c.getAttribute('r')) || running + 1;
      running = col;
      if (col > LIMITS.excelCols) { droppedCols = true; continue; }
      if (hiddenCols.has(col)) continue;
      const cell = readCell(c, env);
      if (cell.value === '' && !cell.formula) continue;
      if (cell.value === '') cell.value = tr('xNoFormulaValue');
      cells.set(col, cell);
      if (col > maxCol) maxCol = col;
    }
    if (cells.size) records.push({ rowNumber: row.getAttribute('r') || String(i + 1), cells });
  }
  if (!records.length) return `--- ${tr('xSheet')}: ${name} | ${tr('xEmpty')} ---`;

  const headers = {};
  for (let c = 1; c <= maxCol; c++) {
    const head = records[0].cells.get(c);
    headers[c] = String(head ? head.value : `${tr('xColumn')} ${columnName(c)}`).slice(0, 100);
  }
  const lines = records.map((record) => {
    const parts = [];
    for (const [col, cell] of Array.from(record.cells).sort((a, b) => a[0] - b[0])) {
      parts.push(`${columnName(col)}=${cell.value}${cell.formula ? ` [${tr('xFormula')}: =${cell.formula}]` : ''}`);
    }
    return `${tr('xRow')} ${record.rowNumber}: ${parts.join(' | ')}`;
  });
  let block = `--- ${tr('xSheet')}: ${name} | ${tr('xSheetHead', { rows: rowTotal, cols: maxCol })} ---\n`;
  block += `${tr('xOverview')}\n`;
  block += `${columnStats(records, maxCol, headers).join('\n') || tr('xNoStats')}\n\n${tr('xRows')}\n${lines.join('\n')}`;
  if (rowTotal > LIMITS.excelRows) {
    report.truncatedRows += rowTotal - LIMITS.excelRows;
    block += `\n${tr('xCutRows', { limit: LIMITS.excelRows, total: rowTotal })}`;
  }
  if (droppedCols) {
    report.truncatedCols = true;
    block += `\n${tr('xCutCols', { limit: LIMITS.excelCols })}`;
  }
  return block;
}

async function extractXlsx(file, options) {
  const zip = await readZip(file);
  const mainPath = await findMainPart(zip, 'xl/workbook.xml');
  const workbookText = await zip.readText(mainPath);
  if (!workbookText) throw new KbError('errXlsxNoSheets');
  const workbook = parseXml(workbookText);
  const dir = dirOf(mainPath);
  const rels = await readRelationships(zip, relsPathOf(mainPath));
  const shared = [];
  const sharedText = await zip.readText(`${dir}/sharedStrings.xml`);
  if (sharedText) for (const si of allByName(parseXml(sharedText), 'si')) shared.push(sharedStringText(si));
  const pr = allByName(workbook, 'workbookPr')[0];
  const flag = pr ? pr.getAttribute('date1904') : null;
  const env = { shared, dateStyles: await readExcelStyles(zip, dir), date1904: flag === '1' || flag === 'true' };
  const report = { hiddenRows: 0, hiddenColumns: 0, hiddenSheets: 0, truncatedRows: 0, truncatedCols: false, skippedSheets: 0 };

  const sheets = [];
  let chars = 0;
  for (const node of allByName(workbook, 'sheet')) {
    const name = safeName(node.getAttribute('name') || tr('nameSheet'));
    const rel = rels.get(relationshipId(node));
    if (!rel) continue;
    if (chars > LIMITS.extractedPerFile) { report.skippedSheets++; continue; }
    const xml = await zip.readText(resolveTarget(dir, rel.target));
    if (!xml) continue;
    const state = node.getAttribute('state');
    const hidden = state === 'hidden' || state === 'veryHidden';
    if (hidden) report.hiddenSheets++;
    const text = excelSheetText(name, xml, env, options, report);
    chars += text.length;
    sheets.push({ name, text, hidden });
    await yieldToUi();
  }
  if (!sheets.length) throw new KbError('errXlsxNoSheets');
  return { sheets, format: 'Excel', report };
}

// ---------------------------------------------------------------- PowerPoint
function slideText(doc) {
  const out = [];
  const walk = (node) => {
    for (const child of Array.from(node.children)) {
      if (child.localName === 'Fallback') continue;
      if (child.localName === 'p') {
        const text = allByName(child, 't').map((t) => t.textContent).join('').trim();
        if (text) out.push(text);
      } else walk(child);
    }
  };
  walk(doc.documentElement);
  return out;
}

async function extractPptx(file, options) {
  const zip = await readZip(file);
  const mainPath = await findMainPart(zip, 'ppt/presentation.xml');
  const dir = dirOf(mainPath);
  const main = await zip.readText(mainPath);
  const rels = await readRelationships(zip, relsPathOf(mainPath));
  let paths = [];
  if (main) {
    for (const id of allByName(parseXml(main), 'sldId')) {
      const rel = rels.get(relationshipId(id));
      if (rel) paths.push(resolveTarget(dir, rel.target));
    }
  }
  if (!paths.length) {
    paths = zip.names().filter((n) => /^ppt\/slides\/slide\d+\.xml$/i.test(n))
      .sort((a, b) => Number(/(\d+)\.xml/.exec(a)[1]) - Number(/(\d+)\.xml/.exec(b)[1]));
  }
  if (!paths.length) throw new KbError('errPptxNoSlides');
  const report = { hiddenSlides: 0, notes: 0, truncatedSlides: Math.max(0, paths.length - LIMITS.pptSlides) };
  const output = [];
  for (let i = 0; i < Math.min(paths.length, LIMITS.pptSlides); i++) {
    const xml = await zip.readText(paths[i]);
    if (!xml) continue;
    const doc = parseXml(xml);
    const hidden = doc.documentElement.getAttribute('show') === '0';
    if (hidden) {
      report.hiddenSlides++;
      if (!options.includeHidden) continue;
    }
    let block = `--- ${tr('xSlide')} ${i + 1}${hidden ? ` (${tr('xHiddenMark')})` : ''} ---\n${slideText(doc).join('\n')}`;
    const slideRels = await readRelationships(zip, relsPathOf(paths[i]));
    for (const rel of slideRels.values()) {
      if (!/\/notesSlide$/.test(rel.type)) continue;
      const notesXml = await zip.readText(resolveTarget(dirOf(paths[i]), rel.target));
      if (!notesXml) continue;
      const notes = slideText(parseXml(notesXml)).filter((t) => !/^\d+$/.test(t));
      if (!notes.length) continue;
      report.notes++;
      if (options.includeHidden) block += `\n[${tr('xNotes')}]\n${notes.join('\n')}`;
    }
    output.push(block);
  }
  return { text: output.join('\n\n'), format: 'PowerPoint PPTX', report };
}

// ---- from Erlaf: 42-pdf.js
// Local PDF text extraction. Not a renderer: it reads objects, follows the page tree and
// interprets only the text operators. Rules it must keep:
//   * every loop and recursion is bounded, because the file is untrusted;
//   * image streams are never decompressed, so they cannot exhaust the budget;
//   * text that cannot be decoded is marked, never passed off as readable text;
//   * pages without text are reported, never skipped silently.
class PdfName { constructor(name) { this.name = name; } }
class PdfRef { constructor(num) { this.num = num; } }
class PdfStr { constructor(raw) { this.raw = raw; } }
class PdfOp { constructor(op) { this.op = op; } }
class PdfTooDeep extends Error {}

const PDF_WS = new Set([0, 9, 10, 12, 13, 32]);
const PDF_DELIM = new Set([40, 41, 60, 62, 91, 93, 123, 125, 47, 37]);

class PdfLexer {
  constructor(text, pos) {
    this.s = text;
    this.pos = pos || 0;
  }

  skip() {
    const s = this.s;
    for (;;) {
      const c = s.charCodeAt(this.pos);
      if (PDF_WS.has(c)) this.pos++;
      else if (c === 37) {
        while (this.pos < s.length && s[this.pos] !== '\n' && s[this.pos] !== '\r') this.pos++;
      } else return;
    }
  }

  word() {
    const s = this.s;
    const start = this.pos;
    while (this.pos < s.length) {
      const c = s.charCodeAt(this.pos);
      if (PDF_WS.has(c) || PDF_DELIM.has(c)) break;
      this.pos++;
    }
    return s.slice(start, this.pos);
  }

  literal() {
    const s = this.s;
    let depth = 1;
    let out = '';
    this.pos++;
    while (this.pos < s.length) {
      const ch = s[this.pos++];
      if (ch === '\\') {
        const n = s[this.pos++];
        if (n === undefined) break;
        const simple = { n: '\n', r: '\r', t: '\t', b: '\b', f: '\f' }[n];
        if (simple) out += simple;
        else if (n === '\r') { if (s[this.pos] === '\n') this.pos++; } else if (n === '\n') { /* line continuation */ } else if (n >= '0' && n <= '7') {
          let oct = n;
          while (oct.length < 3 && s[this.pos] >= '0' && s[this.pos] <= '7') oct += s[this.pos++];
          out += String.fromCharCode(parseInt(oct, 8) & 255);
        } else out += n;
      } else if (ch === '(') { depth++; out += ch; } else if (ch === ')') {
        if (--depth === 0) break;
        out += ch;
      } else out += ch;
    }
    return new PdfStr(out);
  }

  hex() {
    const end = this.s.indexOf('>', this.pos);
    const stop = end < 0 ? this.s.length : end;
    let clean = this.s.slice(this.pos + 1, stop).replace(/[^0-9A-Fa-f]/g, '');
    if (clean.length % 2) clean += '0';
    this.pos = stop + 1;
    let out = '';
    for (let i = 0; i < clean.length; i += 2) out += String.fromCharCode(parseInt(clean.slice(i, i + 2), 16));
    return new PdfStr(out);
  }

  name() {
    this.pos++;
    return new PdfName(this.word().replace(/#([0-9A-Fa-f]{2})/g, (_, h) => String.fromCharCode(parseInt(h, 16))));
  }

  array(depth) {
    this.pos++;
    const out = [];
    for (;;) {
      this.skip();
      if (this.pos >= this.s.length) return out;
      if (this.s[this.pos] === ']') { this.pos++; return out; }
      const value = this.value(depth + 1);
      if (value === undefined) return out;
      if (!(value instanceof PdfOp)) out.push(value);
    }
  }

  dict(depth) {
    this.pos += 2;
    const out = new Map();
    for (;;) {
      this.skip();
      if (this.pos >= this.s.length) return out;
      if (this.s.startsWith('>>', this.pos)) { this.pos += 2; return out; }
      if (this.s[this.pos] !== '/') {
        const before = this.pos;
        this.value(depth + 1);
        if (this.pos === before) this.pos++;
        continue;
      }
      const key = this.name().name;
      this.skip();
      if (this.s.startsWith('>>', this.pos)) continue;
      out.set(key, this.value(depth + 1));
    }
  }

  // Returns undefined at end of input. Unknown words come back as PdfOp.
  value(depth) {
    if (depth > LIMITS.pdfDepth) throw new PdfTooDeep();
    this.skip();
    const s = this.s;
    if (this.pos >= s.length) return undefined;
    const c = s[this.pos];
    if (c === '/') return this.name();
    if (c === '(') return this.literal();
    if (c === '<') return s[this.pos + 1] === '<' ? this.dict(depth) : this.hex();
    if (c === '[') return this.array(depth);
    if (c === ']' || c === '>' || c === ')' || c === '{' || c === '}') {
      this.pos += s[this.pos + 1] === '>' && c === '>' ? 2 : 1;
      return new PdfOp(c);
    }
    const word = this.word();
    if (word === '') { this.pos++; return new PdfOp(''); }
    if (/^[+-]?(\d+\.?\d*|\.\d+)$/.test(word)) {
      if (/^\d+$/.test(word)) {
        const m = /^[ \t\r\n\f\0]+(\d+)[ \t\r\n\f\0]+R(?![A-Za-z0-9])/.exec(s.slice(this.pos, this.pos + 24));
        if (m) {
          this.pos += m[0].length;
          return new PdfRef(Number(word));
        }
      }
      return Number(word);
    }
    if (word === 'true') return true;
    if (word === 'false') return false;
    if (word === 'null') return null;
    return new PdfOp(word);
  }
}

function pdfLatin1(bytes) {
  const parts = [];
  const step = 0x8000;
  for (let i = 0; i < bytes.length; i += step) {
    parts.push(String.fromCharCode.apply(null, bytes.subarray(i, i + step)));
  }
  return parts.join('');
}

function pdfBytes(raw, start, end) {
  const out = new Uint8Array(end - start);
  for (let i = 0; i < out.length; i++) out[i] = raw.charCodeAt(start + i) & 255;
  return out;
}

// ---- Stream filters
function pdfAscii85(bytes) {
  const out = [];
  let group = [];
  for (const b of bytes) {
    if (b === 126) break;
    if (b === 122 && !group.length) { out.push(0, 0, 0, 0); continue; }
    if (b < 33 || b > 117) continue;
    group.push(b - 33);
    if (group.length === 5) {
      let n = 0;
      for (const v of group) n = n * 85 + v;
      out.push(Math.floor(n / 16777216) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255);
      group = [];
    }
  }
  if (group.length > 1) {
    const count = group.length;
    while (group.length < 5) group.push(84);
    let n = 0;
    for (const v of group) n = n * 85 + v;
    out.push(...[Math.floor(n / 16777216) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255].slice(0, count - 1));
  }
  return Uint8Array.from(out);
}

function pdfAsciiHex(bytes) {
  const text = pdfLatin1(bytes).split('>')[0].replace(/[^0-9A-Fa-f]/g, '');
  const padded = text.length % 2 ? `${text}0` : text;
  const out = new Uint8Array(padded.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(padded.slice(i * 2, i * 2 + 2), 16);
  return out;
}

function pdfRunLength(bytes, limit) {
  const out = [];
  let i = 0;
  while (i < bytes.length && out.length < limit) {
    const n = bytes[i++];
    if (n === 128) break;
    if (n < 128) {
      for (let k = 0; k <= n && i < bytes.length; k++) out.push(bytes[i++]);
    } else {
      const v = bytes[i++];
      for (let k = 0; k < 257 - n; k++) out.push(v);
    }
  }
  return Uint8Array.from(out);
}

function pdfLzw(bytes, earlyChange, limit) {
  const chunks = [];
  let size = 0;
  let table = [];
  const reset = () => {
    table = [];
    for (let i = 0; i < 256; i++) table[i] = Uint8Array.of(i);
    table[256] = null;
    table[257] = null;
  };
  reset();
  let width = 9;
  let bitBuf = 0;
  let bits = 0;
  let prev = null;
  for (const b of bytes) {
    bitBuf = ((bitBuf << 8) | b) & 0xffffff;
    bits += 8;
    if (bits < width) continue;
    bits -= width;
    const code = (bitBuf >>> bits) & ((1 << width) - 1);
    if (code === 257) break;
    if (code === 256) { reset(); width = 9; prev = null; continue; }
    let entry;
    if (code < table.length && table[code]) entry = table[code];
    else if (prev) {
      entry = new Uint8Array(prev.length + 1);
      entry.set(prev);
      entry[prev.length] = prev[0];
    } else break;
    chunks.push(entry);
    size += entry.length;
    if (size > limit) throw new KbError('errExpandLimit');
    if (prev && table.length < 4096) {
      const next = new Uint8Array(prev.length + 1);
      next.set(prev);
      next[prev.length] = entry[0];
      table.push(next);
    }
    prev = entry;
    const n = table.length + earlyChange;
    width = n >= 2048 ? 12 : n >= 1024 ? 11 : n >= 512 ? 10 : 9;
  }
  return concatChunks(chunks, size);
}

function pdfPredictor(data, parms) {
  const predictor = Number(parms?.get('Predictor') ?? 1);
  if (predictor < 2) return data;
  const colors = Number(parms.get('Colors') ?? 1);
  const bpc = Number(parms.get('BitsPerComponent') ?? 8);
  const columns = Number(parms.get('Columns') ?? 1);
  const rowBytes = Math.ceil((colors * bpc * columns) / 8);
  const bpp = Math.max(1, Math.ceil((colors * bpc) / 8));
  if (!(rowBytes > 0)) return data;
  if (predictor === 2) {
    if (bpc !== 8) return data;
    const out = data.slice();
    for (let row = 0; row + rowBytes <= out.length; row += rowBytes) {
      for (let i = bpp; i < rowBytes; i++) out[row + i] = (out[row + i] + out[row + i - bpp]) & 255;
    }
    return out;
  }
  const rows = Math.floor(data.length / (rowBytes + 1));
  const out = new Uint8Array(rows * rowBytes);
  for (let r = 0; r < rows; r++) {
    const type = data[r * (rowBytes + 1)];
    const src = r * (rowBytes + 1) + 1;
    const dst = r * rowBytes;
    for (let i = 0; i < rowBytes; i++) {
      const left = i >= bpp ? out[dst + i - bpp] : 0;
      const up = r ? out[dst - rowBytes + i] : 0;
      const upLeft = r && i >= bpp ? out[dst - rowBytes + i - bpp] : 0;
      let add = 0;
      if (type === 1) add = left;
      else if (type === 2) add = up;
      else if (type === 3) add = (left + up) >> 1;
      else if (type === 4) {
        const p = left + up - upLeft;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - upLeft);
        add = pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft;
      }
      out[dst + i] = (data[src + i] + add) & 255;
    }
  }
  return out;
}

// ---- Character decoding
const MAC_ROMAN_HIGH = 'ÄÅÇÉÑÖÜáàâäãåçéèêëíìîïñóòôöõúùûü†°¢£§•¶ß®©™´¨≠ÆØ∞±≤≥¥µ∂∑∏π∫ªºΩæø¿¡¬√ƒ≈∆«»… ' +
  'ÀÃÕŒœ–—“”‘’÷◊ÿŸ⁄€‹›fifl‡·‚„‰ÂÊÁËÈÍÎÏÌÓÔ�ÒÚÛÙıˆ˜¯˘˙˚¸˝˛ˇ';

const WIN_ANSI_TABLE = (() => {
  const all = new Uint8Array(256);
  for (let i = 0; i < 256; i++) all[i] = i;
  return Array.from(new TextDecoder('windows-1252').decode(all));
})();

const MAC_ROMAN_TABLE = (() => {
  const table = WIN_ANSI_TABLE.slice(0, 128);
  // "fi" and "fl" above stand for one code each, so expand the string with care.
  const high = Array.from(MAC_ROMAN_HIGH.replace('fifl', '\ufb01\ufb02'));
  return table.concat(high);
})();

const GLYPH_SPECIAL = Object.freeze({
  space: ' ', exclam: '!', quotedbl: '"', numbersign: '#', dollar: '$', percent: '%', ampersand: '&',
  quotesingle: "'", parenleft: '(', parenright: ')', asterisk: '*', plus: '+', comma: ',', hyphen: '-',
  minus: '-', period: '.', slash: '/', colon: ':', semicolon: ';', less: '<', equal: '=', greater: '>',
  question: '?', at: '@', bracketleft: '[', backslash: '\\', bracketright: ']', asciicircum: '^',
  underscore: '_', grave: '`', braceleft: '{', bar: '|', braceright: '}', asciitilde: '~',
  zero: '0', one: '1', two: '2', three: '3', four: '4', five: '5', six: '6', seven: '7', eight: '8', nine: '9',
  germandbls: 'ß', AE: 'Æ', ae: 'æ', OE: 'Œ', oe: 'œ', Oslash: 'Ø', oslash: 'ø', Euro: '€', fi: 'fi', fl: 'fl',
  ff: 'ff', ffi: 'ffi', ffl: 'ffl', endash: '–', emdash: '—', quoteleft: '‘', quoteright: '’',
  quotedblleft: '“', quotedblright: '”', quotedblbase: '„', quotesinglbase: '‚', guillemotleft: '«',
  guillemotright: '»', guilsinglleft: '‹', guilsinglright: '›', bullet: '•', ellipsis: '…', section: '§',
  paragraph: '¶', degree: '°', plusminus: '±', multiply: '×', divide: '÷', currency: '¤', cent: '¢',
  sterling: '£', yen: '¥', copyright: '©', registered: '®', trademark: '™', nbspace: ' ', dagger: '†',
});
const GLYPH_MARKS = Object.freeze({
  acute: '\u0301', grave: '\u0300', circumflex: '\u0302', tilde: '\u0303', dieresis: '\u0308',
  ring: '\u030a', cedilla: '\u0327', caron: '\u030c',
});

function glyphToUnicode(name) {
  if (Object.prototype.hasOwnProperty.call(GLYPH_SPECIAL, name)) return GLYPH_SPECIAL[name];
  if (/^[A-Za-z]$/.test(name)) return name;
  const uni = /^uni([0-9A-Fa-f]{4})/.exec(name) || /^u([0-9A-Fa-f]{4,6})$/.exec(name);
  if (uni) {
    const cp = parseInt(uni[1], 16);
    return cp <= 0x10ffff ? String.fromCodePoint(cp) : null;
  }
  const accented = /^([A-Za-z])(acute|grave|circumflex|tilde|dieresis|ring|cedilla|caron)$/.exec(name);
  if (accented) return (accented[1] + GLYPH_MARKS[accented[2]]).normalize('NFC');
  return null;
}

function utf16beFromHex(hex) {
  let out = '';
  for (let i = 0; i + 4 <= hex.length; i += 4) out += String.fromCharCode(parseInt(hex.slice(i, i + 4), 16));
  if (hex.length <= 2) out = String.fromCharCode(parseInt(hex, 16));
  return out;
}

function parseCMap(text) {
  const map = new Map();
  const lengths = new Set();
  let budget = 300000;
  const add = (code, value) => { if (budget-- > 0) map.set(code, value); };
  for (const block of text.matchAll(/begincodespacerange([\s\S]*?)endcodespacerange/g)) {
    for (const m of block[1].matchAll(/<([0-9A-Fa-f]+)>\s*<[0-9A-Fa-f]+>/g)) lengths.add(m[1].length / 2);
  }
  for (const block of text.matchAll(/beginbfchar([\s\S]*?)endbfchar/g)) {
    for (const m of block[1].matchAll(/<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f\s]*)>/g)) {
      add(parseInt(m[1], 16), utf16beFromHex(m[2].replace(/\s/g, '')));
    }
  }
  for (const block of text.matchAll(/beginbfrange([\s\S]*?)endbfrange/g)) {
    for (const m of block[1].matchAll(/<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>\s*(?:<([0-9A-Fa-f\s]*)>|\[([^\]]*)\])/g)) {
      const start = parseInt(m[1], 16);
      const end = parseInt(m[2], 16);
      if (!(end >= start) || end - start > 65535) continue;
      if (m[4] !== undefined) {
        const items = Array.from(m[4].matchAll(/<([0-9A-Fa-f\s]*)>/g));
        items.slice(0, end - start + 1).forEach((d, i) => add(start + i, utf16beFromHex(d[1].replace(/\s/g, ''))));
      } else {
        const dest = utf16beFromHex(m[3].replace(/\s/g, ''));
        if (!dest) continue;
        const head = dest.slice(0, -1);
        const tail = dest.charCodeAt(dest.length - 1);
        for (let c = start; c <= end; c++) add(c, head + String.fromCharCode(tail + c - start));
      }
    }
  }
  return { map, lengths: Array.from(lengths).sort((a, b) => b - a) };
}

// ---- Document model
class PdfDocument {
  constructor(raw) {
    this.raw = raw;
    this.objects = new Map();
    this.trailer = new Map();
    this.decoded = 0;
    this.fonts = new Map();
    this.unmapped = 0;
    this.mapped = 0;
  }

  resolve(value) {
    let v = value;
    for (let i = 0; i < 32 && v instanceof PdfRef; i++) v = this.objects.get(v.num)?.value;
    return v instanceof PdfRef ? undefined : v;
  }

  get(dict, key) {
    return dict instanceof Map ? this.resolve(dict.get(key)) : undefined;
  }

  nameOf(value) {
    const v = this.resolve(value);
    return v instanceof PdfName ? v.name : undefined;
  }

  scan() {
    const raw = this.raw;
    const header = /(\d{1,10})[ \t\r\n\f\0]{1,8}(\d{1,5})[ \t\r\n\f\0]{1,8}obj(?![A-Za-z0-9])/g;
    let m;
    while ((m = header.exec(raw)) !== null) {
      const lex = new PdfLexer(raw, header.lastIndex);
      let value;
      try {
        value = lex.value(0);
      } catch (e) {
        if (e instanceof PdfTooDeep) continue;
        throw e;
      }
      if (value instanceof PdfOp) continue;
      const entry = { value, stream: null, pos: m.index };
      lex.skip();
      if (value instanceof Map && raw.startsWith('stream', lex.pos)) {
        let start = lex.pos + 6;
        if (raw[start] === '\r' && raw[start + 1] === '\n') start += 2;
        else if (raw[start] === '\n' || raw[start] === '\r') start += 1;
        let end = -1;
        const declared = value.get('Length');
        if (typeof declared === 'number' && declared >= 0) {
          const after = new PdfLexer(raw, start + declared);
          after.skip();
          if (raw.startsWith('endstream', after.pos)) end = start + declared;
        }
        if (end < 0) {
          end = raw.indexOf('endstream', start);
          if (end < 0) end = raw.length;
          if (raw[end - 1] === '\n') end--;
          if (raw[end - 1] === '\r') end--;
        }
        entry.stream = { start, end: Math.max(start, end) };
        header.lastIndex = end;
      } else header.lastIndex = Math.max(header.lastIndex, lex.pos);
      this.objects.set(Number(m[1]), entry);
    }
    for (const t of raw.matchAll(/trailer/g)) {
      try {
        const dict = new PdfLexer(raw, t.index + 7).value(0);
        if (dict instanceof Map) for (const [k, v] of dict) this.trailer.set(k, v);
      } catch (e) {
        if (!(e instanceof PdfTooDeep)) throw e;
      }
    }
    for (const entry of this.objects.values()) {
      if (entry.stream && this.nameOf(entry.value.get('Type')) === 'XRef') {
        for (const k of ['Root', 'Encrypt']) if (entry.value.has(k)) this.trailer.set(k, entry.value.get(k));
      }
    }
  }

  async expandObjectStreams() {
    const containers = Array.from(this.objects.values())
      .filter((e) => e.stream && this.nameOf(e.value.get('Type')) === 'ObjStm')
      .sort((a, b) => a.pos - b.pos);
    for (const container of containers) {
      const data = await this.streamBytes(container);
      if (!data) continue;
      const text = pdfLatin1(data);
      const count = Number(this.get(container.value, 'N'));
      const first = Number(this.get(container.value, 'First'));
      if (!Number.isInteger(count) || !Number.isInteger(first) || count > 100000) continue;
      const head = new PdfLexer(text, 0);
      const index = [];
      for (let i = 0; i < count; i++) {
        const num = head.value(0);
        const offset = head.value(0);
        if (typeof num !== 'number' || typeof offset !== 'number') break;
        index.push([num, offset]);
      }
      for (const [num, offset] of index) {
        const existing = this.objects.get(num);
        if (existing && existing.pos > container.pos) continue;
        try {
          const value = new PdfLexer(text, first + offset).value(0);
          if (!(value instanceof PdfOp)) this.objects.set(num, { value, stream: null, pos: container.pos });
        } catch (e) {
          if (!(e instanceof PdfTooDeep)) throw e;
        }
      }
    }
  }

  // Returns the decoded stream, or null if it is image data or cannot be decoded.
  async streamBytes(entry) {
    if (entry.bytes !== undefined) return entry.bytes;
    entry.bytes = null;
    const out = await this.decode(entry, false);
    entry.bytes = out ? out.data : null;
    return entry.bytes;
  }

  // For page images: stops before a picture codec, so a JPEG comes out as JPEG bytes.
  imageBytes(entry) {
    return this.decode(entry, true);
  }

  async decode(entry, image) {
    if (!entry.stream) return null;
    let filters = this.get(entry.value, 'Filter') ?? this.get(entry.value, 'F');
    let parms = this.get(entry.value, 'DecodeParms') ?? this.get(entry.value, 'DP');
    filters = filters === undefined || filters === null ? [] : Array.isArray(filters) ? filters : [filters];
    parms = Array.isArray(parms) ? parms : [parms];
    let data = pdfBytes(this.raw, entry.stream.start, entry.stream.end);
    const limit = () => Math.min(LIMITS.pdfStreamBytes, LIMITS.pdfTotalBytes - this.decoded);
    for (let i = 0; i < filters.length; i++) {
      const name = this.nameOf(filters[i]);
      const parm = this.resolve(parms[i]);
      const dict = parm instanceof Map ? new Map(Array.from(parm, ([k, v]) => [k, this.resolve(v)])) : null;
      if (name === 'FlateDecode' || name === 'Fl') {
        try {
          data = await inflate(data, 'deflate', limit(), true);
        } catch (e) {
          if (e.key === 'errExpandLimit') throw e;
          try {
            data = await inflate(data.subarray(2), 'deflate-raw', limit(), true);
          } catch (e2) {
            if (e2.key === 'errExpandLimit') throw e2;
            return null;
          }
        }
        data = pdfPredictor(data, dict);
      } else if (name === 'LZWDecode' || name === 'LZW') {
        data = pdfPredictor(pdfLzw(data, Number(dict?.get('EarlyChange') ?? 1), limit()), dict);
      } else if (name === 'ASCII85Decode' || name === 'A85') data = pdfAscii85(data);
      else if (name === 'ASCIIHexDecode' || name === 'AHx') data = pdfAsciiHex(data);
      else if (name === 'RunLengthDecode' || name === 'RL') data = pdfRunLength(data, limit());
      else if (image && PDF_IMAGE_CODECS.has(name)) {
        this.decoded += data.length;
        return { data, codec: PDF_IMAGE_CODECS.get(name) };
      } else if (name !== 'Crypt') return null;
    }
    this.decoded += data.length;
    return { data, codec: null };
  }

  async font(ref) {
    const key = ref instanceof PdfRef ? ref.num : ref;
    if (this.fonts.has(key)) return this.fonts.get(key);
    const dict = this.resolve(ref);
    const font = await this.buildFont(dict instanceof Map ? dict : new Map());
    this.fonts.set(key, font);
    return font;
  }

  async buildFont(dict) {
    const composite = this.nameOf(dict.get('Subtype')) === 'Type0';
    const base = this.nameOf(dict.get('BaseFont')) || '';
    const font = { composite, cmap: null, lengths: [2], table: null, symbolic: false, widths: null, defaultWidth: composite ? 1000 : 500 };
    const toUnicode = dict.get('ToUnicode');
    const cmapEntry = toUnicode instanceof PdfRef ? this.objects.get(toUnicode.num) : null;
    if (cmapEntry) {
      const data = await this.streamBytes(cmapEntry);
      if (data) {
        const parsed = parseCMap(pdfLatin1(data));
        if (parsed.map.size) font.cmap = parsed.map;
        if (composite && parsed.lengths.length) font.lengths = parsed.lengths;
      }
    }
    if (composite) {
      const descendant = this.resolve(this.get(dict, 'DescendantFonts')?.[0]);
      if (descendant instanceof Map) {
        const dw = this.get(descendant, 'DW');
        if (typeof dw === 'number') font.defaultWidth = dw;
        font.widths = this.compositeWidths(this.get(descendant, 'W'));
      }
      return font;
    }
    font.symbolic = /Symbol|Dingbats|Wingdings|CMSY|CMEX|MSAM|MSBM/i.test(base);
    const encoding = this.resolve(dict.get('Encoding'));
    const baseName = encoding instanceof PdfName ? encoding.name :
      encoding instanceof Map ? this.nameOf(encoding.get('BaseEncoding')) : undefined;
    font.table = (baseName === 'MacRomanEncoding' ? MAC_ROMAN_TABLE : WIN_ANSI_TABLE).slice();
    if (encoding instanceof Map) {
      const diffs = this.get(encoding, 'Differences');
      if (Array.isArray(diffs)) {
        let code = 0;
        for (const item of diffs) {
          const v = this.resolve(item);
          if (typeof v === 'number') code = v;
          else if (v instanceof PdfName) {
            if (code >= 0 && code < 256) font.table[code] = glyphToUnicode(v.name) ?? '�';
            code++;
          }
        }
        font.custom = true;
      }
    }
    const widths = this.get(dict, 'Widths');
    const first = this.get(dict, 'FirstChar');
    if (Array.isArray(widths) && typeof first === 'number') {
      font.widths = new Map();
      widths.forEach((w, i) => {
        const v = this.resolve(w);
        if (typeof v === 'number') font.widths.set(first + i, v);
      });
    }
    return font;
  }

  compositeWidths(w) {
    if (!Array.isArray(w)) return null;
    const map = new Map();
    let i = 0;
    while (i < w.length && map.size < 70000) {
      const first = this.resolve(w[i]);
      const second = this.resolve(w[i + 1]);
      if (Array.isArray(second)) {
        second.forEach((v, k) => { const n = this.resolve(v); if (typeof n === 'number') map.set(first + k, n); });
        i += 2;
      } else {
        const width = this.resolve(w[i + 2]);
        if (typeof first === 'number' && typeof second === 'number' && typeof width === 'number') {
          for (let c = first; c <= second && c - first < 70000; c++) map.set(c, width);
        }
        i += 3;
      }
    }
    return map;
  }

  // Decodes one shown string. Returns its text and its width in text-space units.
  showString(font, raw, state) {
    let out = '';
    let width = 0;
    const count = (code, text) => {
      width += (font?.widths?.get(code) ?? font?.defaultWidth ?? 500) / 1000 * state.size + state.tc;
      if (text === ' ') width += state.tw;
      out += text;
    };
    if (!font) {
      for (let i = 0; i < raw.length; i++) {
        this.mapped++;
        count(raw.charCodeAt(i), WIN_ANSI_TABLE[raw.charCodeAt(i)]);
      }
      return { text: out, width };
    }
    if (font.composite) {
      let i = 0;
      while (i < raw.length) {
        let matched = false;
        for (const len of font.lengths) {
          if (i + len > raw.length) continue;
          let code = 0;
          for (let k = 0; k < len; k++) code = code * 256 + raw.charCodeAt(i + k);
          if (font.cmap && font.cmap.has(code)) {
            this.mapped++;
            count(code, font.cmap.get(code));
            i += len;
            matched = true;
            break;
          }
        }
        if (!matched) {
          const len = font.lengths[font.lengths.length - 1];
          let code = 0;
          for (let k = 0; k < len && i + k < raw.length; k++) code = code * 256 + raw.charCodeAt(i + k);
          this.unmapped++;
          count(code, '�');
          i += len;
        }
      }
      return { text: out, width };
    }
    for (let i = 0; i < raw.length; i++) {
      const code = raw.charCodeAt(i);
      const mappedChar = font.cmap ? font.cmap.get(code) : undefined;
      if (mappedChar !== undefined) { this.mapped++; count(code, mappedChar); continue; }
      if (font.symbolic) { this.mapped++; count(code, '•'); continue; }
      const ch = font.table[code];
      // A control code here means a font-specific encoding this table does not describe.
      if (ch === undefined || ch === '�' || code < 32) { this.unmapped++; count(code, '�'); } else { this.mapped++; count(code, ch); }
    }
    return { text: out, width };
  }
}

const matMul = (a, b) => [
  a[0] * b[0] + a[1] * b[2], a[0] * b[1] + a[1] * b[3],
  a[2] * b[0] + a[3] * b[2], a[2] * b[1] + a[3] * b[3],
  a[4] * b[0] + a[5] * b[2] + b[4], a[4] * b[1] + a[5] * b[3] + b[5],
];

function pdfTextString(value) {
  if (value instanceof PdfName) return value.name;
  if (!(value instanceof PdfStr)) return '';
  const r = value.raw;
  const utf16 = (start, little) => {
    let out = '';
    for (let i = start; i + 1 < r.length; i += 2) {
      out += String.fromCharCode(little ? r.charCodeAt(i) | (r.charCodeAt(i + 1) << 8) : (r.charCodeAt(i) << 8) | r.charCodeAt(i + 1));
    }
    return out;
  };
  if (r.startsWith('\u00fe\u00ff')) return utf16(2, false);
  if (r.startsWith('\u00ff\u00fe')) return utf16(2, true);
  if (r.startsWith('\u00ef\u00bb\u00bf')) {
    return new TextDecoder().decode(pdfBytes(r, 3, r.length));
  }
  return r;
}

// Runs one content stream, appending to page.out. Unknown operators are ignored.
async function runContent(pdf, text, resources, page, inherited, depth) {
  const state = {
    ctm: inherited, stack: [], font: null, size: 12, tc: 0, tw: 0, tz: 1, leading: 0,
    tm: [1, 0, 0, 1, 0, 0], lm: [1, 0, 0, 1, 0, 0],
  };
  const lex = new PdfLexer(text, 0);
  let args = [];

  const nextLine = (tx, ty) => {
    state.lm = matMul([1, 0, 0, 1, tx, ty], state.lm);
    state.tm = state.lm.slice();
  };
  const show = (raw) => {
    const { text: decoded, width } = pdf.showString(state.font, raw, state);
    const m = matMul(state.tm, state.ctm);
    const scale = Math.hypot(m[2], m[3]) || 1;
    const fontSize = Math.abs(state.size) * scale;
    if (decoded) {
      if (page.lastY !== null) {
        const dy = Math.abs(m[5] - page.lastY);
        if (dy > Math.max(fontSize * 0.45, 0.5)) {
          if (!page.out.endsWith('\n')) page.out += '\n';
        } else if (m[4] - page.endX > fontSize * 0.18 && !/\s$/.test(page.out) && !decoded.startsWith(' ')) {
          page.out += ' ';
        }
      }
      page.out += decoded;
      page.lastY = m[5];
    }
    state.tm = matMul([1, 0, 0, 1, width * state.tz, 0], state.tm);
    const after = matMul(state.tm, state.ctm);
    page.endX = after[4];
  };

  for (;;) {
    let token;
    try {
      token = lex.value(0);
    } catch (e) {
      if (e instanceof PdfTooDeep) return; // keep the text gathered so far
      throw e;
    }
    if (token === undefined) return;
    if (!(token instanceof PdfOp)) {
      args.push(token);
      if (args.length > 64) args.shift();
      continue;
    }
    const n = (i) => (typeof args[i] === 'number' ? args[i] : 0);
    switch (token.op) {
      case 'q':
        if (state.stack.length < 256) state.stack.push([state.ctm, state.font, state.size]);
        break;
      case 'Q': {
        const saved = state.stack.pop();
        if (saved) [state.ctm, state.font, state.size] = saved;
        break;
      }
      case 'cm':
        state.ctm = matMul([n(0), n(1), n(2), n(3), n(4), n(5)], state.ctm);
        break;
      case 'BT':
        state.tm = [1, 0, 0, 1, 0, 0];
        state.lm = [1, 0, 0, 1, 0, 0];
        break;
      case 'Tf': {
        const fonts = pdf.get(resources, 'Font');
        const name = args[0] instanceof PdfName ? args[0].name : '';
        state.font = fonts instanceof Map && fonts.has(name) ? await pdf.font(fonts.get(name)) : null;
        state.size = n(1);
        break;
      }
      case 'Tc': state.tc = n(0); break;
      case 'Tw': state.tw = n(0); break;
      case 'Tz': state.tz = n(0) / 100; break;
      case 'TL': state.leading = n(0); break;
      case 'Td': nextLine(n(0), n(1)); break;
      case 'TD': state.leading = -n(1); nextLine(n(0), n(1)); break;
      case 'Tm':
        state.lm = [n(0), n(1), n(2), n(3), n(4), n(5)];
        state.tm = state.lm.slice();
        break;
      case 'T*': nextLine(0, -state.leading); break;
      case 'Tj':
        if (args[0] instanceof PdfStr) show(args[0].raw);
        break;
      case "'":
        nextLine(0, -state.leading);
        if (args[0] instanceof PdfStr) show(args[0].raw);
        break;
      case '"':
        state.tw = n(0);
        state.tc = n(1);
        nextLine(0, -state.leading);
        if (args[2] instanceof PdfStr) show(args[2].raw);
        break;
      case 'TJ':
        if (Array.isArray(args[0])) {
          for (const item of args[0]) {
            if (item instanceof PdfStr) show(item.raw);
            else if (typeof item === 'number') {
              state.tm = matMul([1, 0, 0, 1, (-item / 1000) * state.size * state.tz, 0], state.tm);
            }
          }
        }
        break;
      case 'BI': {
        // Inline image: its binary data must not be read as operators.
        const id = /[\s]ID[\s]/g;
        id.lastIndex = lex.pos - 1;
        const start = id.exec(text);
        if (!start) return;
        const end = /[\s]EI(?=[\s]|$)/g;
        end.lastIndex = id.lastIndex;
        const stop = end.exec(text);
        if (!stop) return;
        lex.pos = end.lastIndex;
        page.hasImage = true;
        break;
      }
      case 'Do': {
        const xobjects = pdf.get(resources, 'XObject');
        const name = args[0] instanceof PdfName ? args[0].name : '';
        const ref = xobjects instanceof Map ? xobjects.get(name) : undefined;
        const entry = ref instanceof PdfRef ? pdf.objects.get(ref.num) : null;
        if (!entry || !(entry.value instanceof Map)) break;
        const subtype = pdf.nameOf(entry.value.get('Subtype'));
        if (subtype === 'Image') {
          page.hasImage = true;
          if (page.images.length < 16) page.images.push(entry);
          break;
        }
        if (subtype !== 'Form' || depth >= 8 || page.active.has(ref.num)) break;
        const data = await pdf.streamBytes(entry);
        if (!data) break;
        page.active.add(ref.num);
        const matrix = pdf.get(entry.value, 'Matrix');
        const ctm = Array.isArray(matrix) && matrix.length === 6 ?
          matMul(matrix.map((v) => Number(pdf.resolve(v)) || 0), state.ctm) : state.ctm;
        const inner = pdf.get(entry.value, 'Resources');
        await runContent(pdf, pdfLatin1(data), inner instanceof Map ? inner : resources, page, ctm, depth + 1);
        page.active.delete(ref.num);
        break;
      }
      default:
        break;
    }
    args = [];
  }
}

function collectPages(pdf) {
  const pages = [];
  const seen = new Set();
  const walk = (ref, inherited, depth) => {
    if (depth > LIMITS.pdfDepth || pages.length >= LIMITS.pdfPages) return;
    if (ref instanceof PdfRef) {
      if (seen.has(ref.num)) return;
      seen.add(ref.num);
    }
    const node = pdf.resolve(ref);
    if (!(node instanceof Map)) return;
    const resources = pdf.get(node, 'Resources');
    const scope = resources instanceof Map ? resources : inherited;
    const kids = pdf.get(node, 'Kids');
    if (Array.isArray(kids)) {
      for (const kid of kids) walk(kid, scope, depth + 1);
    } else if (pdf.nameOf(node.get('Type')) !== 'Pages') pages.push({ node, resources: scope });
  };
  const root = pdf.resolve(pdf.trailer.get('Root'));
  const catalog = root instanceof Map ? root :
    Array.from(pdf.objects.values()).map((e) => e.value)
      .find((v) => v instanceof Map && pdf.nameOf(v.get('Type')) === 'Catalog');
  if (catalog instanceof Map) walk(catalog.get('Pages'), null, 0);
  if (!pages.length) {
    // Damaged page tree: fall back to any page objects, in file order.
    for (const entry of pdf.objects.values()) {
      if (entry.value instanceof Map && pdf.nameOf(entry.value.get('Type')) === 'Page') {
        let resources = pdf.get(entry.value, 'Resources');
        let parent = pdf.get(entry.value, 'Parent');
        for (let i = 0; i < 16 && !(resources instanceof Map) && parent instanceof Map; i++) {
          resources = pdf.get(parent, 'Resources');
          parent = pdf.get(parent, 'Parent');
        }
        pages.push({ node: entry.value, resources: resources instanceof Map ? resources : null });
        if (pages.length >= LIMITS.pdfPages) break;
      }
    }
  }
  return { pages, catalog };
}

function collectFormFields(pdf, catalog) {
  const form = pdf.get(catalog, 'AcroForm');
  const fields = pdf.get(form, 'Fields');
  if (!Array.isArray(fields)) return [];
  const out = [];
  const seen = new Set();
  const walk = (ref, prefix, depth) => {
    if (depth > 16 || out.length > 2000) return;
    if (ref instanceof PdfRef) {
      if (seen.has(ref.num)) return;
      seen.add(ref.num);
    }
    const node = pdf.resolve(ref);
    if (!(node instanceof Map)) return;
    const part = pdfTextString(pdf.get(node, 'T'));
    const name = [prefix, part].filter(Boolean).join('.');
    const kids = pdf.get(node, 'Kids');
    if (Array.isArray(kids)) for (const kid of kids) walk(kid, name, depth + 1);
    const value = pdfTextString(pdf.get(node, 'V'));
    if (value && value !== 'Off') out.push(`${name}: ${value}`);
  };
  for (const f of fields) walk(f, '', 0);
  return out;
}

async function extractPdf(file) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  const raw = pdfLatin1(bytes);
  if (!raw.slice(0, 1024).includes('%PDF-')) throw new KbError('errPdfHeader');
  const pdf = new PdfDocument(raw);
  pdf.scan();
  if (!pdf.objects.size) throw new KbError('errPdfNoObjects');
  if (pdf.trailer.has('Encrypt')) throw new KbError('errPdfEncrypted');
  await pdf.expandObjectStreams();

  const { pages, catalog } = collectPages(pdf);
  if (!pages.length) throw new KbError('errPdfNoObjects');
  const blocks = [];
  const pagesWithoutText = [];
  let anyImage = false;
  const scanned = [];
  let chars = 0;
  let truncatedPages = 0;
  for (let i = 0; i < pages.length; i++) {
    if (chars > LIMITS.extractedPerFile) { truncatedPages = pages.length - i; break; }
    const { node, resources } = pages[i];
    const contents = pdf.get(node, 'Contents');
    const list = Array.isArray(contents) ? contents : [node.get('Contents')];
    const page = { out: '', lastY: null, endX: 0, hasImage: false, images: [], active: new Set() };
    // Content arrays are one logical stream; a split may fall anywhere between tokens.
    const parts = [];
    for (const ref of list) {
      const entry = ref instanceof PdfRef ? pdf.objects.get(ref.num) : null;
      const data = entry ? await pdf.streamBytes(entry) : null;
      if (data) parts.push(pdfLatin1(data));
    }
    await runContent(pdf, parts.join('\n'), resources, page, [1, 0, 0, 1, 0, 0], 0);
    if (page.hasImage) anyImage = true;
    const text = page.out.replace(/[\ufb00-\ufb04]/g, (c) => c.normalize('NFKC')).replace(/[ \t]+\n/g, '\n').trim();
    if (text) {
      blocks.push(`--- ${tr('xPage')} ${i + 1} ---\n${text}`);
      chars += text.length;
    } else {
      pagesWithoutText.push(i + 1);
      if (page.images.length) scanned.push({ page: i + 1, images: page.images });
    }
    if (i % 8 === 7) await yieldToUi();
  }

  const fields = catalog instanceof Map ? collectFormFields(pdf, catalog) : [];
  if (fields.length) blocks.push(`--- ${tr('xFormFields')} ---\n${fields.join('\n')}`);
  const { pageImages, unreadable } = await scannedPageImages(pdf, scanned);
  if (pageImages.length) blocks.push(tr('xScannedPages', { pages: pageImages.map((p) => p.page).join(', ') }));
  if (!blocks.length) throw new KbError(anyImage ? 'errPdfScanned' : 'errPdfNoText');
  const total = pdf.mapped + pdf.unmapped;
  if (total && pdf.unmapped / total > 0.25) throw new KbError('errPdfGarbled');
  const report = {
    pages: pages.length, pagesWithoutText: unreadable, scanPages: pageImages.length, unmappedGlyphs: pdf.unmapped, truncatedPages, formFields: fields.length,
  };
  return { text: blocks.join('\n\n'), format: 'PDF', report, pageImages };
}

// Scanned pages. Erlaf has no OCR. Instead the largest picture on a page without text goes to
// the image model as a JPEG. JPEG pages pass through, raw pixel data is drawn on a canvas.
// CCITT, JBIG2 and JPEG 2000 cannot be decoded by the browser and are reported as unreadable.
const PDF_IMAGE_CODECS = new Map([
  ['DCTDecode', 'jpeg'], ['DCT', 'jpeg'], ['JPXDecode', 'jpx'], ['CCITTFaxDecode', 'ccitt'], ['CCF', 'ccitt'], ['JBIG2Decode', 'jbig2'],
]);
const SCAN_MAX_SIDE = 2000;

async function scannedPageImages(pdf, scanned) {
  const pageImages = [];
  const unreadable = [];
  let bytes = 0;
  for (const { page, images } of scanned) {
    const size = (e) => (Number(pdf.get(e.value, 'Width')) || 0) * (Number(pdf.get(e.value, 'Height')) || 0);
    const largest = images.reduce((a, b) => (size(b) > size(a) ? b : a));
    let dataUrl = null;
    if (pageImages.length < LIMITS.scanPages) {
      try {
        dataUrl = await pdfImageDataUrl(pdf, largest);
      } catch (e) {
        if (e instanceof KbError && e.key === 'errExpandLimit') throw e;
      }
    }
    const length = dataUrl ? Math.ceil((dataUrl.length * 3) / 4) : 0;
    if (dataUrl && bytes + length <= LIMITS.imagesTotalBytes) {
      pageImages.push({ page, dataUrl, bytes: length });
      bytes += length;
    } else unreadable.push(page);
    await yieldToUi();
  }
  return { pageImages, unreadable };
}

async function pdfImageDataUrl(pdf, entry) {
  const dict = entry.value;
  const width = Number(pdf.get(dict, 'Width')) || 0;
  const height = Number(pdf.get(dict, 'Height')) || 0;
  if (width < 16 || height < 16 || width * height > 60e6) return null;
  const decoded = await pdf.imageBytes(entry);
  if (!decoded || !decoded.data) return null;
  if (decoded.codec === 'jpeg') {
    const jpeg = decoded.data;
    if (jpeg[0] !== 0xff || jpeg[1] !== 0xd8) return null;
    if (Math.max(width, height) <= SCAN_MAX_SIDE && jpeg.length <= LIMITS.imageBytes / 2) {
      return `data:image/jpeg;base64,${bytesToBase64(jpeg)}`;
    }
    const bitmap = await createImageBitmap(new Blob([jpeg], { type: 'image/jpeg' }));
    return canvasJpeg(width, height, (ctx) => ctx.drawImage(bitmap, 0, 0, width, height));
  }
  if (decoded.codec) return null;
  const pixels = pdfPixels(pdf, dict, decoded.data, width, height);
  if (!pixels) return null;
  return canvasJpeg(width, height, (ctx) => ctx.putImageData(new ImageData(pixels, width, height), 0, 0));
}

// Raw samples to RGBA: 8-bit gray, RGB and CMYK, and 1-bit gray as scanners write it.
function pdfPixels(pdf, dict, data, width, height) {
  const bpc = Number(pdf.get(dict, 'BitsPerComponent')) || 8;
  if (pdf.get(dict, 'ImageMask') === true) return null;
  const space = pdf.resolve(dict.get('ColorSpace'));
  let comps = 0;
  if (Array.isArray(space) && pdf.nameOf(space[0]) === 'ICCBased') {
    const n = Number(pdf.get(pdf.resolve(space[1]), 'N'));
    comps = [1, 3, 4].includes(n) ? n : 0;
  } else {
    const name = pdf.nameOf(space);
    comps = { DeviceGray: 1, G: 1, CalGray: 1, DeviceRGB: 3, RGB: 3, CalRGB: 3, DeviceCMYK: 4, CMYK: 4 }[name] ?? 0;
  }
  if (!comps) return null;
  const decode = pdf.get(dict, 'Decode');
  const invert = Array.isArray(decode) && Number(pdf.resolve(decode[0])) === 1;
  const out = new Uint8ClampedArray(width * height * 4);
  if (bpc === 8) {
    if (data.length < width * height * comps) return null;
    for (let i = 0, o = 0; i < width * height; i++, o += 4) {
      const s = i * comps;
      let r;
      let g;
      let b;
      if (comps === 1) r = g = b = data[s];
      else if (comps === 3) { r = data[s]; g = data[s + 1]; b = data[s + 2]; } else {
        const k = 255 - data[s + 3];
        r = ((255 - data[s]) * k) / 255; g = ((255 - data[s + 1]) * k) / 255; b = ((255 - data[s + 2]) * k) / 255;
      }
      if (invert) { r = 255 - r; g = 255 - g; b = 255 - b; }
      out[o] = r; out[o + 1] = g; out[o + 2] = b; out[o + 3] = 255;
    }
    return out;
  }
  if (bpc === 1 && comps === 1) {
    const row = Math.ceil(width / 8);
    if (data.length < row * height) return null;
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const bit = (data[y * row + (x >> 3)] >> (7 - (x & 7))) & 1;
        const v = (bit ^ (invert ? 1 : 0)) ? 255 : 0;
        const o = (y * width + x) * 4;
        out[o] = v; out[o + 1] = v; out[o + 2] = v; out[o + 3] = 255;
      }
    }
    return out;
  }
  return null;
}

// Draws at full size, then scales so the longer side is at most SCAN_MAX_SIDE, as JPEG.
async function canvasJpeg(width, height, draw) {
  const full = el('canvas', { width, height });
  draw(full.getContext('2d'));
  const scale = Math.min(1, SCAN_MAX_SIDE / Math.max(width, height));
  let target = full;
  if (scale < 1) {
    target = el('canvas', { width: Math.round(width * scale), height: Math.round(height * scale) });
    const ctx = target.getContext('2d');
    ctx.imageSmoothingQuality = 'high';
    ctx.drawImage(full, 0, 0, target.width, target.height);
  }
  const blob = await new Promise((resolve) => target.toBlob(resolve, 'image/jpeg', 0.85));
  if (!blob) return null;
  return `data:image/jpeg;base64,${bytesToBase64(new Uint8Array(await blob.arrayBuffer()))}`;
}

// ---- from Erlaf: 43-formats.js
// Further readable formats: OpenDocument, email (.eml) and RTF.

// ---- OpenDocument. Tracked changes and annotations are left out, as with Word.
function odfText(node) {
  let out = '';
  for (const child of Array.from(node.childNodes)) {
    if (child.nodeType === 3) out += child.textContent;
    else if (child.nodeType === 1) {
      const name = child.localName;
      if (name === 's') out += ' '.repeat(Math.min(Number(child.getAttribute('text:c')) || 1, 200));
      else if (name === 'tab') out += '\t';
      else if (name === 'line-break') out += '\n';
      else if (name !== 'annotation' && name !== 'tracked-changes') out += odfText(child);
    }
  }
  return out;
}

function odfBlocks(container, report) {
  const blocks = [];
  for (const child of Array.from(container.children)) {
    const name = child.localName;
    if (name === 'p' || name === 'h') {
      report.comments += allByName(child, 'annotation').length;
      const text = odfText(child).trimEnd();
      if (text) blocks.push(text);
    } else if (name === 'table') {
      const rows = allByName(child, 'table-row').map((r) => Array.from(r.children)
        .filter((c) => c.localName === 'table-cell').map((c) => odfBlocks(c, report).join(' / ')).join(' | '));
      if (rows.length) blocks.push(`[${tr('xTable')}]\n${rows.join('\n')}\n[/${tr('xTable')}]`);
    } else if (name === 'tracked-changes') report.trackedDeletions += allByName(child, 'deletion').length;
    else if (['list', 'list-item', 'list-header', 'section'].includes(name)) blocks.push(...odfBlocks(child, report));
  }
  return blocks;
}

async function readOdfContent(file) {
  const zip = await readZip(file);
  const xml = await zip.readText('content.xml');
  if (!xml) throw new KbError('errNotOffice');
  return parseXml(xml);
}

async function extractOdt(file) {
  const doc = await readOdfContent(file);
  const body = allByName(doc, 'text')[0];
  if (!body) throw new KbError('errDocxNoMain');
  const report = { trackedDeletions: 0, comments: 0 };
  return { text: odfBlocks(body, report).join('\n\n'), format: tr('xOdfText'), report };
}

function odsSheet(table, report) {
  const name = safeName(table.getAttribute('table:name') || tr('nameSheet'));
  const lines = [];
  let rowNumber = 0;
  for (const row of allByName(table, 'table-row')) {
    // Empty rows are often stored as one element repeated a million times.
    const repeat = Number(row.getAttribute('table:number-rows-repeated')) || 1;
    if (row.getAttribute('table:visibility') === 'collapse') {
      report.hiddenRows += Math.min(repeat, LIMITS.excelRows);
      rowNumber += repeat;
      continue;
    }
    let col = 0;
    const cells = [];
    for (const cell of Array.from(row.children)) {
      if (cell.localName !== 'table-cell' && cell.localName !== 'covered-table-cell') continue;
      const span = Number(cell.getAttribute('table:number-columns-repeated')) || 1;
      const text = odfBlocks(cell, report).join(' ');
      if (text) {
        for (let k = 0; k < Math.min(span, 50) && col + k < LIMITS.excelCols; k++) cells.push(`${columnName(col + k + 1)}=${text}`);
      }
      col += span;
    }
    rowNumber += 1;
    if (cells.length) {
      if (lines.length >= LIMITS.excelRows) { report.truncatedRows++; continue; }
      lines.push(`${tr('xRow')} ${rowNumber}: ${cells.join(' | ')}`);
    }
    rowNumber += repeat - 1;
  }
  return { name, text: `--- ${tr('xSheet')}: ${name} ---\n${lines.join('\n') || `[${tr('xEmpty')}]`}`, hidden: false };
}

async function extractOds(file) {
  const doc = await readOdfContent(file);
  const report = { hiddenRows: 0, truncatedRows: 0, comments: 0, trackedDeletions: 0 };
  const sheets = allByName(doc, 'table').filter((t) => t.parentNode.localName === 'spreadsheet').map((t) => odsSheet(t, report));
  if (!sheets.length) throw new KbError('errXlsxNoSheets');
  return { sheets, format: tr('xOdfSheet'), report };
}

async function extractOdp(file) {
  const doc = await readOdfContent(file);
  const report = { notes: 0, comments: 0, trackedDeletions: 0 };
  const slides = allByName(doc, 'page').filter((p) => p.localName === 'page' && p.parentNode.localName === 'presentation');
  if (!slides.length) throw new KbError('errPptxNoSlides');
  const out = slides.map((slide, i) => {
    const lines = [];
    const walk = (node) => {
      for (const child of Array.from(node.children)) {
        if (child.localName === 'notes') {
          if (allByName(child, 'p').some((p) => odfText(p).trim())) report.notes++;
        } else if (child.localName === 'p') {
          const text = odfText(child).trim();
          if (text) lines.push(text);
        } else walk(child);
      }
    };
    walk(slide);
    return `--- ${tr('xSlide')} ${i + 1} ---\n${lines.join('\n')}`;
  });
  return { text: out.join('\n\n'), format: tr('xOdfSlides'), report };
}

// ---- Email. Only the message text is read; attachments are named, not opened.
function decodeBytes(bytes, charset) {
  try {
    return new TextDecoder(charset || 'utf-8').decode(bytes);
  } catch (_) {
    return new TextDecoder('windows-1252').decode(bytes);
  }
}

function binaryBytes(text) {
  const out = new Uint8Array(text.length);
  for (let i = 0; i < text.length; i++) out[i] = text.charCodeAt(i) & 255;
  return out;
}

function quotedPrintable(text) {
  return text.replace(/=\r?\n/g, '').replace(/=([0-9A-Fa-f]{2})/g, (_, h) => String.fromCharCode(parseInt(h, 16)));
}

function decodeBase64Loose(text) {
  const clean = text.replace(/[^A-Za-z0-9+/]/g, '');
  const usable = clean.slice(0, clean.length - (clean.length % 4 === 1 ? 1 : 0));
  try {
    return atob(usable.padEnd(Math.ceil(usable.length / 4) * 4, '='));
  } catch (_) {
    return '';
  }
}

// RFC 2047 encoded words, as in subjects: =?utf-8?Q?Gr=C3=BC=C3=9Fe?=
function decodeHeader(value) {
  return value.replace(/\?=\s+=\?/g, '?==?').replace(/=\?([^?]+)\?([bBqQ])\?([^?]*)\?=/g, (_, charset, kind, text) => {
    const raw = kind.toLowerCase() === 'b' ? decodeBase64Loose(text) : quotedPrintable(text.replace(/_/g, ' '));
    return decodeBytes(binaryBytes(raw), charset);
  });
}

function parseMimeHeaders(block) {
  const headers = new Map();
  for (const line of block.replace(/\r?\n[ \t]+/g, ' ').split(/\r?\n/)) {
    const at = line.indexOf(':');
    if (at > 0) headers.set(line.slice(0, at).toLowerCase().trim(), line.slice(at + 1).trim());
  }
  return headers;
}

const mimeParam = (value, name) => {
  const m = new RegExp(`${name}\\*?=(?:"([^"]*)"|([^;\\s]+))`, 'i').exec(value || '');
  return m ? m[1] ?? m[2] : '';
};

function splitMime(raw) {
  const m = /\r?\n\r?\n/.exec(raw);
  if (!m) return { headers: parseMimeHeaders(raw), body: '' };
  return { headers: parseMimeHeaders(raw.slice(0, m.index)), body: raw.slice(m.index + m[0].length) };
}

function walkMime(part, depth, found) {
  const type = (part.headers.get('content-type') || 'text/plain').toLowerCase();
  const disposition = part.headers.get('content-disposition') || '';
  if (type.startsWith('multipart/') && depth < 8) {
    const boundary = mimeParam(part.headers.get('content-type'), 'boundary');
    if (boundary) {
      for (const chunk of part.body.split(`--${boundary}`).slice(1)) {
        if (chunk.startsWith('--')) break;
        walkMime(splitMime(chunk.replace(/^\r?\n/, '')), depth + 1, found);
      }
    }
    return;
  }
  const fileName = mimeParam(disposition, 'filename') || mimeParam(part.headers.get('content-type'), 'name');
  if (/^attachment/i.test(disposition) || fileName) {
    found.attachments.push(decodeHeader(fileName) || tr('nameUntitled'));
    return;
  }
  if (!type.startsWith('text/plain') && !type.startsWith('text/html')) return;
  const encoding = (part.headers.get('content-transfer-encoding') || '').toLowerCase();
  const raw = encoding === 'base64' ? decodeBase64Loose(part.body) : encoding === 'quoted-printable' ? quotedPrintable(part.body) : part.body;
  const text = decodeBytes(binaryBytes(raw), mimeParam(part.headers.get('content-type'), 'charset'));
  if (type.startsWith('text/plain') && !found.plain) found.plain = text;
  else if (type.startsWith('text/html') && !found.html) found.html = text;
}

async function extractEml(file) {
  const raw = pdfLatin1(new Uint8Array(await file.arrayBuffer()));
  const message = splitMime(raw);
  const found = { plain: '', html: '', attachments: [] };
  walkMime(message, 0, found);
  // DOMParser builds an inert document: no scripts run and no remote images load.
  const htmlText = () => {
    const doc = new DOMParser().parseFromString(found.html, 'text/html');
    for (const n of doc.querySelectorAll('script, style')) n.remove();
    return doc.body ? doc.body.innerText ?? doc.body.textContent : '';
  };
  const body = found.plain || (found.html ? htmlText() : '');
  const head = [[tr('xFrom'), 'from'], [tr('xTo'), 'to'], ['Cc', 'cc'], [tr('xDate'), 'date'], [tr('xSubject'), 'subject']]
    .filter(([, k]) => message.headers.has(k)).map(([label, k]) => `${label}: ${decodeHeader(message.headers.get(k))}`);
  return {
    text: `${head.join('\n')}\n\n${body.trim()}`, format: tr('xEmail'),
    report: { attachments: found.attachments.length, attachmentNames: found.attachments.join(', ').slice(0, 200) },
  };
}

// ---- RTF
const RTF_SKIP = new Set(['fonttbl', 'colortbl', 'stylesheet', 'info', 'pict', 'object', 'themedata', 'colorschememapping', 'datastore', 'latentstyles', 'listtable', 'listoverridetable', 'rsidtbl', 'generator', 'xmlnstbl']);

async function extractRtf(file) {
  const s = pdfLatin1(new Uint8Array(await file.arrayBuffer()));
  if (!s.startsWith('{\\rtf')) throw new KbError('errBinaryFile');
  const cp1252 = new TextDecoder('windows-1252');
  const stack = [];
  let group = { skip: false, hidden: false, uc: 1 };
  let out = '';
  let hiddenRuns = 0;
  let bytes = [];
  let pendingSkip = 0;
  const emit = (text) => { if (!group.skip && !group.hidden) out += text; };
  const flush = () => {
    if (bytes.length) emit(cp1252.decode(Uint8Array.from(bytes)));
    bytes = [];
  };
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (c === '{') {
      flush();
      if (stack.length > 200) throw new KbError('errZipCorrupt');
      stack.push(group);
      group = { ...group };
      i++;
    } else if (c === '}') {
      flush();
      group = stack.pop() || group;
      i++;
    } else if (c === '\\') {
      const next = s[i + 1];
      if (next === "'") {
        const v = parseInt(s.slice(i + 2, i + 4), 16);
        i += 4;
        if (pendingSkip > 0) pendingSkip--;
        else if (Number.isFinite(v)) bytes.push(v);
      } else if (next === '*') {
        group.skip = true;
        i += 2;
      } else if (next === '\\' || next === '{' || next === '}') {
        flush();
        emit(next);
        i += 2;
      } else if (next === '~') {
        flush();
        emit(' ');
        i += 2;
      } else if (next === '\n' || next === '\r') {
        flush();
        emit('\n');
        i += 2;
      } else {
        const m = /^([a-zA-Z]{1,32})(-?\d{1,10})? ?/.exec(s.slice(i + 1, i + 50));
        if (!m) { i += 2; continue; }
        i += 1 + m[0].length;
        flush();
        const word = m[1];
        const value = m[2] === undefined ? null : Number(m[2]);
        if (RTF_SKIP.has(word)) group.skip = true;
        else if (word === 'par' || word === 'line' || word === 'row' || word === 'sect' || word === 'page') emit('\n');
        else if (word === 'tab') emit('\t');
        else if (word === 'cell') emit(' | ');
        else if (word === 'uc') group.uc = value ?? 1;
        else if (word === 'v') {
          group.hidden = value !== 0;
          if (group.hidden) hiddenRuns++;
        } else if (word === 'bin') i += Math.max(0, value ?? 0);
        else if (word === 'u') {
          emit(String.fromCharCode((value ?? 0) < 0 ? (value + 65536) : (value ?? 0)));
          pendingSkip = group.uc;
        }
      }
    } else if (c === '\n' || c === '\r') i++;
    else {
      if (pendingSkip > 0) pendingSkip--;
      else bytes.push(s.charCodeAt(i) & 255);
      i++;
    }
  }
  flush();
  return { text: out.replace(/\n{3,}/g, '\n\n').trim(), format: 'RTF', report: { hiddenText: hiddenRuns } };
}

// ---- from Erlaf: 44-msg.js
// Outlook .msg reader. A .msg is an OLE compound file: a small file system inside a file, so
// this walks its sector chains. Every chain is bounded and cycle-checked, as the file is untrusted.
const CFB_NONE = 0xffffffff;
const CFB_END = 0xfffffffe;

function readCfb(bytes) {
  if (bytes.length < 512 || bytes[0] !== 0xd0 || bytes[1] !== 0xcf || bytes[2] !== 0x11 || bytes[3] !== 0xe0) throw new KbError('errBinaryFile');
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.length);
  const u32 = (o) => view.getUint32(o, true);
  const shift = view.getUint16(30, true);
  if (shift !== 9 && shift !== 12) throw new KbError('errZipCorrupt');
  const sectorSize = 1 << shift;
  const miniSize = 1 << view.getUint16(32, true);
  const miniCutoff = u32(56);
  const sectorCount = Math.floor(bytes.length / sectorSize);
  const offsetOf = (n) => (n + 1) * sectorSize;
  const valid = (n) => n < sectorCount && offsetOf(n) + sectorSize <= bytes.length;

  // The FAT sector list starts in the header and may continue in a chain of its own.
  const fatSectors = [];
  for (let i = 0; i < 109; i++) {
    const s = u32(76 + i * 4);
    if (s < CFB_END && valid(s)) fatSectors.push(s);
  }
  const seenDifat = new Set();
  for (let s = u32(68); s < CFB_END && valid(s) && !seenDifat.has(s); s = u32(offsetOf(s) + sectorSize - 4)) {
    seenDifat.add(s);
    for (let i = 0; i < sectorSize / 4 - 1; i++) {
      const f = u32(offsetOf(s) + i * 4);
      if (f < CFB_END && valid(f)) fatSectors.push(f);
    }
  }
  const perSector = sectorSize / 4;
  const fat = new Uint32Array(fatSectors.length * perSector);
  fatSectors.forEach((s, i) => {
    for (let k = 0; k < perSector; k++) fat[i * perSector + k] = u32(offsetOf(s) + k * 4);
  });

  function chain(start, table, limit) {
    const out = [];
    const seen = new Set();
    for (let s = start; s < CFB_END && s < table.length; s = table[s]) {
      if (seen.has(s) || out.length >= limit) throw new KbError('errZipCorrupt');
      seen.add(s);
      out.push(s);
    }
    return out;
  }

  function readSectors(start, size) {
    const sectors = chain(start, fat, sectorCount + 1).filter(valid);
    const out = new Uint8Array(sectors.length * sectorSize);
    sectors.forEach((s, i) => out.set(bytes.subarray(offsetOf(s), offsetOf(s) + sectorSize), i * sectorSize));
    return size === undefined ? out : out.subarray(0, Math.min(size, out.length));
  }

  const directory = readSectors(u32(48));
  const dirView = new DataView(directory.buffer);
  const entries = [];
  for (let o = 0; o + 128 <= directory.length && entries.length < 100000; o += 128) {
    const nameLength = Math.min(dirView.getUint16(o + 64, true), 64);
    entries.push({
      name: new TextDecoder('utf-16le').decode(directory.subarray(o, o + Math.max(0, nameLength - 2))),
      type: directory[o + 66], left: dirView.getUint32(o + 68, true), right: dirView.getUint32(o + 72, true),
      child: dirView.getUint32(o + 76, true), start: dirView.getUint32(o + 116, true), size: dirView.getUint32(o + 120, true),
    });
  }
  const root = entries[0];
  if (!root) throw new KbError('errZipCorrupt');
  const miniData = readSectors(root.start, root.size);
  const miniFatBytes = readSectors(u32(60));
  const miniFat = new Uint32Array(Math.floor(miniFatBytes.length / 4));
  const miniView = new DataView(miniFatBytes.buffer);
  for (let i = 0; i < miniFat.length; i++) miniFat[i] = miniView.getUint32(i * 4, true);

  function readStream(entry) {
    if (entry.size > LIMITS.zipEntryBytes) throw new KbError('errExpandLimit');
    if (entry.size >= miniCutoff) return readSectors(entry.start, entry.size);
    const sectors = chain(entry.start, miniFat, Math.ceil(entry.size / miniSize) + 1);
    const out = new Uint8Array(sectors.length * miniSize);
    sectors.forEach((s, i) => out.set(miniData.subarray(s * miniSize, (s + 1) * miniSize), i * miniSize));
    return out.subarray(0, entry.size);
  }

  function children(entry) {
    const out = [];
    const seen = new Set();
    const stack = [entry.child];
    while (stack.length) {
      const id = stack.pop();
      if (id === CFB_NONE || id >= entries.length || seen.has(id)) continue;
      seen.add(id);
      const e = entries[id];
      out.push(e);
      stack.push(e.left, e.right);
    }
    return out;
  }
  return { root, children, readStream };
}

async function extractMsg(file) {
  const cfb = readCfb(new Uint8Array(await file.arrayBuffer()));
  const top = cfb.children(cfb.root);
  const text = (list, id) => {
    const unicode = list.find((e) => e.name === `__substg1.0_${id}001F`);
    if (unicode) return new TextDecoder('utf-16le').decode(cfb.readStream(unicode)).replace(/\u0000+$/, '');
    const ansi = list.find((e) => e.name === `__substg1.0_${id}001E`);
    return ansi ? new TextDecoder('windows-1252').decode(cfb.readStream(ansi)).replace(/\u0000+$/, '') : '';
  };
  let body = text(top, '1000');
  if (!body.trim()) {
    const html = top.find((e) => e.name === '__substg1.0_10130102');
    if (html) {
      const doc = new DOMParser().parseFromString(decodeBytes(cfb.readStream(html), 'utf-8'), 'text/html');
      for (const n of doc.querySelectorAll('script, style')) n.remove();
      body = doc.body ? doc.body.textContent : '';
    }
  }
  const attachments = top.filter((e) => e.name.startsWith('__attach_version1.0')).map((a) => {
    const parts = cfb.children(a);
    return text(parts, '3707') || text(parts, '3001') || tr('nameUntitled');
  });
  const head = [[tr('xFrom'), '0C1A'], [tr('xTo'), '0E04'], ['Cc', '0E03'], [tr('xSubject'), '0037']]
    .map(([label, id]) => [label, text(top, id)]).filter(([, v]) => v).map(([label, v]) => `${label}: ${v}`);
  return {
    text: `${head.join('\n')}\n\n${body.trim()}`, format: tr('xOutlook'),
    report: { attachments: attachments.length, attachmentNames: safeName(attachments.join(', ')).slice(0, 200) },
  };
}

// ---- from Erlaf: 45-files.js
// File intake: text decoding, image verification, and the excerpt planner.

// BOM first, then strict UTF-8. German Windows exports are usually Windows-1252, so
// that is the fallback, and the user is told it was used.
function decodeTextBytes(bytes) {
  const b = bytes;
  if (b[0] === 0xef && b[1] === 0xbb && b[2] === 0xbf) return { text: new TextDecoder('utf-8').decode(b.subarray(3)), encoding: 'UTF-8' };
  if (b[0] === 0xff && b[1] === 0xfe) return { text: new TextDecoder('utf-16le').decode(b.subarray(2)), encoding: 'UTF-16' };
  if (b[0] === 0xfe && b[1] === 0xff) return { text: new TextDecoder('utf-16be').decode(b.subarray(2)), encoding: 'UTF-16' };
  const sample = b.subarray(0, 8192);
  let nuls = 0;
  let oddNuls = 0;
  for (let i = 0; i < sample.length; i++) {
    if (sample[i] === 0) {
      nuls++;
      if (i % 2) oddNuls++;
    }
  }
  if (sample.length >= 4 && nuls > sample.length * 0.3) {
    // UTF-16 without BOM: ASCII-range text has a zero in every second byte.
    const little = oddNuls > nuls / 2;
    return { text: new TextDecoder(little ? 'utf-16le' : 'utf-16be').decode(b), encoding: 'UTF-16' };
  }
  if (nuls) throw new KbError('errBinaryFile');
  try {
    return { text: new TextDecoder('utf-8', { fatal: true }).decode(b), encoding: 'UTF-8' };
  } catch (_) {
    return { text: new TextDecoder('windows-1252').decode(b), encoding: 'Windows-1252' };
  }
}

// The file's actual bytes decide the image type. The extension and browser-reported
// type are only hints, and SVG (which can carry scripts) is never accepted.
function sniffImage(b) {
  if (b.length >= 8 && b[0] === 0x89 && b[1] === 0x50 && b[2] === 0x4e && b[3] === 0x47) return 'image/png';
  if (b.length >= 3 && b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff) return 'image/jpeg';
  if (b.length >= 6 && b[0] === 0x47 && b[1] === 0x49 && b[2] === 0x46 && b[3] === 0x38) return 'image/gif';
  if (b.length >= 12 && b[0] === 0x52 && b[1] === 0x49 && b[2] === 0x46 && b[3] === 0x46 &&
      b[8] === 0x57 && b[9] === 0x45 && b[10] === 0x42 && b[11] === 0x50) return 'image/webp';
  return null;
}

function classifyFile(file) {
  const ext = fileExtension(file.name);
  if (IMAGE_EXTENSIONS.has(ext)) return { ext, kind: 'image', max: LIMITS.imageBytes };
  if (TEXT_EXTENSIONS.has(ext)) return { ext, kind: 'text', max: LIMITS.textBytes };
  if (OFFICE_EXTENSIONS.has(ext)) return { ext, kind: 'office', max: LIMITS.officeBytes };
  if (MAIL_EXTENSIONS.has(ext)) return { ext, kind: 'mail', max: LIMITS.officeBytes };
  if (ext === 'pdf') return { ext, kind: 'pdf', max: LIMITS.pdfBytes };
  if (Object.prototype.hasOwnProperty.call(LEGACY_OFFICE, ext)) return { ext, kind: 'legacy', max: LIMITS.officeBytes };
  return null;
}

// Runs the right extractor. The result is { text | sheets | dataUrl, format, report }.
async function extractEntry(entry, options) {
  const { file, kind, ext } = entry;
  if (kind === 'legacy') throw new KbError('errLegacyOffice', { target: LEGACY_OFFICE[ext] });
  if (kind === 'image') {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const mime = sniffImage(bytes);
    if (!mime) throw new KbError('errNotImage');
    return { dataUrl: `data:${mime};base64,${bytesToBase64(bytes)}`, bytes: bytes.length, format: mime };
  }
  if (kind === 'text') {
    if (!file.size) throw new KbError('errEmptyFile');
    const { text, encoding } = decodeTextBytes(new Uint8Array(await file.arrayBuffer()));
    return { text, format: ext.toUpperCase(), report: { encoding } };
  }
  if (kind === 'pdf') return extractPdf(file);
  if (ext === 'eml') return extractEml(file);
  if (ext === 'msg') return extractMsg(file);
  if (ext === 'rtf') return extractRtf(file);
  if (ext === 'odt') return extractOdt(file);
  if (ext === 'ods') return extractOds(file);
  if (ext === 'odp') return extractOdp(file);
  if (ext === 'docx') return extractDocx(file, options);
  if (ext === 'pptx') return extractPptx(file, options);
  return extractXlsx(file, options);
}

function limitExtract(text) {
  if (text.length <= LIMITS.extractedPerFile) return { text, truncated: false };
  return { text: text.slice(0, LIMITS.extractedPerFile), truncated: true };
}

function chunkText(text, size) {
  const chunks = [];
  let start = 0;
  while (start < text.length) {
    let end = Math.min(text.length, start + size);
    if (end < text.length) {
      const cut = text.lastIndexOf('\n', end);
      if (cut > start + size * 0.55) end = cut + 1;
    }
    chunks.push({ start, text: text.slice(start, end) });
    start = end;
  }
  return chunks;
}

// Picks the most relevant chunks within a character budget. With no usable query terms it
// spreads evenly over the document, so a summary at least sees all of it in outline.
function selectExcerpts(text, query, budget) {
  const size = 2800;
  const chunks = chunkText(text, size);
  const count = Math.max(1, Math.min(chunks.length, Math.floor(budget / size)));
  const words = (query.toLowerCase().match(/[\p{L}\p{N}_-]{3,}/gu) || []).filter((w) => !STOPWORDS.has(w));
  const terms = Array.from(new Set(words));
  const scored = chunks.map((chunk, index) => {
    const lower = chunk.text.toLowerCase();
    let score = 0;
    for (const term of terms) {
      let at = 0;
      let hits = 0;
      while (hits < 8 && (at = lower.indexOf(term, at)) >= 0) {
        hits++;
        at += term.length;
      }
      score += hits;
    }
    return { index, score };
  });
  let picked;
  if (scored.some((c) => c.score > 0)) {
    picked = scored.slice().sort((a, b) => b.score - a.score || a.index - b.index).slice(0, count).map((c) => c.index);
  } else {
    picked = Array.from({ length: count }, (_, i) => Math.round((i * (chunks.length - 1)) / Math.max(1, count - 1)));
  }
  picked = Array.from(new Set(picked)).sort((a, b) => a - b);
  return { chunks: picked.map((i) => chunks[i]), total: chunks.length };
}

// ---- from Erlaf: 50-markdown.js
// Small Markdown renderer. It only ever creates DOM nodes from a fixed set of tags and
// assigns text through text nodes, so model output and imported text cannot become markup.
// Links and images are shown as plain text on purpose: a rendered image or clickable link
// in model output could send data out of the page.
function renderInline(parent, text) {
  const pattern = /(`[^`\n]+`|\*\*[^*\n]+\*\*|__[^_\n]+__|\*[^*\n]+\*)/g;
  let last = 0;
  let match;
  while ((match = pattern.exec(text)) !== null) {
    if (match.index > last) parent.append(text.slice(last, match.index));
    const token = match[0];
    if (token[0] === '`') parent.append(el('code', { class: 'inline-code', text: token.slice(1, -1) }));
    else if (token.startsWith('**') || token.startsWith('__')) parent.append(el('strong', { text: token.slice(2, -2) }));
    else parent.append(el('em', { text: token.slice(1, -1) }));
    last = pattern.lastIndex;
  }
  if (last < text.length) parent.append(text.slice(last));
}

const LIST_ITEM = /^(\s*)(?:[-*+]|(\d{1,9})[.)])\s+(.*)$/;

function renderList(root, lines, startIndex) {
  // Nesting uses indentation. A stack of open lists keeps this linear and non-recursive.
  const stack = [];
  let i = startIndex;
  while (i < lines.length) {
    const m = LIST_ITEM.exec(lines[i]);
    if (!m) break;
    const indent = m[1].replace(/\t/g, '    ').length;
    const ordered = m[2] !== undefined;
    while (stack.length && stack[stack.length - 1].indent > indent) stack.pop();
    let top = stack[stack.length - 1];
    if (!top || top.indent < indent || top.ordered !== ordered) {
      if (top && top.indent === indent) stack.pop();
      const list = el(ordered ? 'ol' : 'ul');
      if (ordered && Number(m[2]) !== 1) list.start = Number(m[2]);
      const parent = stack[stack.length - 1];
      if (parent && parent.item) parent.item.append(list);
      else root.append(list);
      top = { indent, ordered, list, item: null };
      stack.push(top);
    }
    const li = el('li');
    renderInline(li, m[3]);
    top.list.append(li);
    top.item = li;
    i++;
  }
  return i;
}

function renderTable(root, lines, startIndex) {
  const cells = (row) => row.trim().replace(/^\|/, '').replace(/\|$/, '').split('|').map((x) => x.trim());
  const heads = cells(lines[startIndex]);
  const headRow = el('tr');
  for (const head of heads) {
    const th = el('th', { scope: 'col' });
    renderInline(th, head);
    headRow.append(th);
  }
  const body = el('tbody');
  let i = startIndex + 2;
  while (i < lines.length && lines[i].includes('|') && lines[i].trim()) {
    const values = cells(lines[i]);
    const tr = el('tr');
    heads.forEach((_, j) => {
      const td = el('td');
      renderInline(td, values[j] ?? '');
      tr.append(td);
    });
    body.append(tr);
    i++;
  }
  root.append(el('div', { class: 'table-scroll', tabIndex: 0 }, el('table', null, el('thead', null, headRow), body)));
  return i;
}

function renderCode(root, code, lang) {
  const pre = el('pre', { tabIndex: 0 }, el('code', { text: code }));
  const copy = toolButton('copy', tr('copy'), () => copyToClipboard(code));
  copy.classList.add('code-copy');
  const head = el('div', { class: 'code-head' }, el('span', { text: lang.slice(0, 40) }), copy);
  root.append(el('div', { class: 'code-block' }, head, pre));
}

function renderMarkdown(root, source) {
  const lines = String(source ?? '').replace(/\r\n?/g, '\n').split('\n');
  let paragraph = [];
  const flush = () => {
    if (!paragraph.length) return;
    const p = el('p');
    paragraph.forEach((line, n) => {
      if (n) p.append(el('br'));
      renderInline(p, line);
    });
    root.append(p);
    paragraph = [];
  };
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const fence = /^\s*(`{3,}|~{3,})(.*)$/.exec(line);
    if (fence) {
      flush();
      const close = new RegExp(`^\\s*${fence[1][0]}{${fence[1].length},}\\s*$`);
      const code = [];
      i++;
      while (i < lines.length && !close.test(lines[i])) code.push(lines[i++]);
      i++;
      renderCode(root, code.join('\n'), fence[2].trim());
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      flush();
      // Chat answers sit below the page's own headings, so levels start at 3.
      const h = el(`h${Math.min(heading[1].length + 2, 6)}`);
      let title = heading[2].trimEnd();
      while (title.endsWith('#')) title = title.slice(0, -1);
      renderInline(h, title.trimEnd());
      root.append(h);
      i++;
      continue;
    }
    if (/^\s{0,3}([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flush();
      root.append(el('hr'));
      i++;
      continue;
    }
    if (/^\s*>/.test(line)) {
      flush();
      const quote = el('blockquote');
      const inner = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) inner.push(lines[i++].replace(/^\s*>\s?/, ''));
      renderInline(quote, inner.join(' '));
      root.append(quote);
      continue;
    }
    if (LIST_ITEM.test(line)) {
      flush();
      i = renderList(root, lines, i);
      continue;
    }
    if (line.includes('|') && i + 1 < lines.length && /^\s*\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)*\|?\s*$/.test(lines[i + 1]) && lines[i + 1].includes('-')) {
      flush();
      i = renderTable(root, lines, i);
      continue;
    }
    if (!line.trim()) flush();
    else paragraph.push(line.trim());
    i++;
  }
  flush();
}

// Reasoning models often emit <think>...</think>. It is shown collapsed and is not sent
// back as history, and copy and export use only the answer itself.
function splitThinking(content) {
  const m = /^\s*<think>([\s\S]*?)(<\/think>\s*|$)/.exec(content);
  if (!m) return { thinking: '', answer: content };
  return { thinking: m[1].trim(), answer: content.slice(m[0].length) };
}

// ---- from Erlaf: 46-export.js
// Word export. Writes a small, valid .docx from the same DOM the chat shows, using a stored
// (uncompressed) ZIP, so no library is needed.
const xmlEscape = (text) => String(text)
  .replace(/[^\t\n\r\u{20}-\u{D7FF}\u{E000}-\u{FFFD}\u{10000}-\u{10FFFF}]/gu, '')
  .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

function zipStore(files) {
  const encoder = new TextEncoder();
  const chunks = [];
  const central = [];
  let offset = 0;
  const u16 = (v) => [v & 255, (v >>> 8) & 255];
  const u32 = (v) => [v & 255, (v >>> 8) & 255, (v >>> 16) & 255, (v >>> 24) & 255];
  for (const [name, text] of files) {
    const nameBytes = encoder.encode(name);
    const data = encoder.encode(text);
    const crc = crc32(data);
    // Fixed date 1980-01-01, so identical content gives identical bytes.
    const common = [...u16(20), ...u16(0x800), ...u16(0), ...u16(0), ...u16(33), ...u32(crc), ...u32(data.length), ...u32(data.length), ...u16(nameBytes.length), ...u16(0)];
    const local = Uint8Array.from([0x50, 0x4b, 3, 4, ...common]);
    chunks.push(local, nameBytes, data);
    central.push(Uint8Array.from([0x50, 0x4b, 1, 2, ...u16(20), ...common, ...u16(0), ...u16(0), ...u16(0), ...u32(0), ...u32(offset)]), nameBytes);
    offset += local.length + nameBytes.length + data.length;
  }
  const centralSize = central.reduce((n, c) => n + c.length, 0);
  const end = Uint8Array.from([0x50, 0x4b, 5, 6, ...u16(0), ...u16(0), ...u16(files.length), ...u16(files.length), ...u32(centralSize), ...u32(offset), ...u16(0)]);
  const all = [...chunks, ...central, end];
  return concatChunks(all, all.reduce((n, c) => n + c.length, 0));
}

function docxRuns(node, format) {
  let out = '';
  for (const child of Array.from(node.childNodes)) {
    if (child.nodeType === 3) {
      if (child.textContent) out += `<w:r>${format ? `<w:rPr>${format}</w:rPr>` : ''}<w:t xml:space="preserve">${xmlEscape(child.textContent)}</w:t></w:r>`;
    } else if (child.nodeType === 1) {
      const tag = child.tagName;
      if (tag === 'STRONG') out += docxRuns(child, `${format}<w:b/>`);
      else if (tag === 'EM') out += docxRuns(child, `${format}<w:i/>`);
      else if (tag === 'CODE') out += docxRuns(child, `<w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Consolas"/>${format}`);
      else if (tag === 'BR') out += '<w:r><w:br/></w:r>';
      else if (tag !== 'UL' && tag !== 'OL') out += docxRuns(child, format);
    }
  }
  return out;
}

const docxParagraph = (runs, props) => `<w:p>${props ? `<w:pPr>${props}</w:pPr>` : ''}${runs}</w:p>`;

function docxList(list, level, state) {
  const ordered = list.tagName === 'OL';
  const numId = state.lists.length + 1;
  state.lists.push({ ordered, start: list.start || 1 });
  let out = '';
  for (const item of Array.from(list.children)) {
    out += docxParagraph(docxRuns(item, ''), `<w:numPr><w:ilvl w:val="${Math.min(level, 8)}"/><w:numId w:val="${numId}"/></w:numPr>`);
    for (const nested of Array.from(item.children)) {
      if (nested.tagName === 'UL' || nested.tagName === 'OL') out += docxList(nested, level + 1, state);
    }
  }
  return out;
}

function docxTable(table) {
  const rows = Array.from(table.querySelectorAll('tr'));
  const columns = Math.max(1, ...rows.map((r) => r.children.length));
  const width = Math.floor(9000 / columns);
  const line = (n) => `<w:${n} w:val="single" w:sz="4" w:space="0" w:color="808080"/>`;
  const borders = ['top', 'left', 'bottom', 'right', 'insideH', 'insideV'].map(line).join('');
  let out = `<w:tbl><w:tblPr><w:tblW w:w="${width * columns}" w:type="dxa"/><w:tblBorders>${borders}</w:tblBorders></w:tblPr><w:tblGrid>`;
  out += `<w:gridCol w:w="${width}"/>`.repeat(columns);
  out += '</w:tblGrid>';
  for (const row of rows) {
    out += '<w:tr>';
    for (const cell of Array.from(row.children)) {
      out += `<w:tc><w:tcPr><w:tcW w:w="${width}" w:type="dxa"/></w:tcPr>${docxParagraph(docxRuns(cell, cell.tagName === 'TH' ? '<w:b/>' : ''))}</w:tc>`;
    }
    out += `${'<w:tc><w:tcPr><w:tcW w:w="0" w:type="auto"/></w:tcPr><w:p/></w:tc>'.repeat(columns - row.children.length)}</w:tr>`;
  }
  return `${out}</w:tbl><w:p/>`;
}

function docxBlocks(root, state) {
  let out = '';
  for (const node of Array.from(root.children)) {
    const tag = node.tagName;
    if (/^H[3-6]$/.test(tag)) out += docxParagraph(docxRuns(node, ''), `<w:pStyle w:val="Heading${Math.min(Number(tag[1]) - 2, 3)}"/>`);
    else if (tag === 'P') out += docxParagraph(docxRuns(node, ''));
    else if (tag === 'UL' || tag === 'OL') out += docxList(node, 0, state);
    else if (tag === 'BLOCKQUOTE') out += docxParagraph(docxRuns(node, '<w:i/>'), '<w:ind w:left="360"/>');
    else if (tag === 'HR') out += '<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="4" w:space="1" w:color="808080"/></w:pBdr></w:pPr></w:p>';
    else if (node.classList.contains('table-scroll')) out += docxTable(node.querySelector('table'));
    else if (node.classList.contains('code-block')) {
      for (const line of node.querySelector('code').textContent.split('\n')) {
        out += docxParagraph(`<w:r><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Consolas"/><w:sz w:val="20"/></w:rPr><w:t xml:space="preserve">${xmlEscape(line)}</w:t></w:r>`, '<w:spacing w:after="0"/>');
      }
    }
  }
  return out;
}

const WML = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"';
const XML_HEAD = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n';

function docxNumbering(lists) {
  const level = (i, ordered) => `<w:lvl w:ilvl="${i}"><w:start w:val="1"/><w:numFmt w:val="${ordered ? 'decimal' : 'bullet'}"/><w:lvlText w:val="${ordered ? `%${i + 1}.` : '•'}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="${360 * (i + 1)}" w:hanging="360"/></w:pPr></w:lvl>`;
  const abstract = (id, ordered) => `<w:abstractNum w:abstractNumId="${id}"><w:multiLevelType w:val="hybridMultilevel"/>${[0, 1, 2, 3, 4, 5, 6, 7, 8].map((i) => level(i, ordered)).join('')}</w:abstractNum>`;
  const nums = lists.map((l, i) => `<w:num w:numId="${i + 1}"><w:abstractNumId w:val="${l.ordered ? 1 : 0}"/>${l.ordered ? `<w:lvlOverride w:ilvl="0"><w:startOverride w:val="${l.start}"/></w:lvlOverride>` : ''}</w:num>`).join('');
  return `${XML_HEAD}<w:numbering ${WML}>${abstract(0, false)}${abstract(1, true)}${nums}</w:numbering>`;
}

function buildDocx(markdown, notice) {
  const root = document.createElement('div');
  renderMarkdown(root, markdown);
  const state = { lists: [] };
  const intro = docxParagraph(`<w:r><w:rPr><w:i/><w:color w:val="666666"/></w:rPr><w:t>${xmlEscape(notice)}</w:t></w:r>`);
  const body = `${intro}${docxBlocks(root, state)}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1417" w:right="1417" w:bottom="1134" w:left="1417" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>`;
  const heading = (n, size) => `<w:style w:type="paragraph" w:styleId="Heading${n}"><w:name w:val="heading ${n}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="80"/><w:outlineLvl w:val="${n - 1}"/></w:pPr><w:rPr><w:b/><w:sz w:val="${size}"/></w:rPr></w:style>`;
  const styles = `${XML_HEAD}<w:styles ${WML}><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/><w:sz w:val="22"/><w:lang w:val="en-GB"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="264" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>${heading(1, 32)}${heading(2, 28)}${heading(3, 24)}</w:styles>`;
  const pkg = 'http://schemas.openxmlformats.org/package/2006';
  const office = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
  const wmlType = 'application/vnd.openxmlformats-officedocument.wordprocessingml';
  return zipStore([
    ['[Content_Types].xml', `${XML_HEAD}<Types xmlns="${pkg}/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="${wmlType}.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="${wmlType}.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="${wmlType}.numbering+xml"/></Types>`],
    ['_rels/.rels', `${XML_HEAD}<Relationships xmlns="${pkg}/relationships"><Relationship Id="rId1" Type="${office}/officeDocument" Target="word/document.xml"/></Relationships>`],
    ['word/_rels/document.xml.rels', `${XML_HEAD}<Relationships xmlns="${pkg}/relationships"><Relationship Id="rId1" Type="${office}/styles" Target="styles.xml"/><Relationship Id="rId2" Type="${office}/numbering" Target="numbering.xml"/></Relationships>`],
    ['word/document.xml', `${XML_HEAD}<w:document ${WML}><w:body>${body}</w:body></w:document>`],
    ['word/styles.xml', styles],
    ['word/numbering.xml', docxNumbering(state.lists)],
  ]);
}

function errorText(e){ return e && e.name === "KbError" ? tr(e.key, e.params) : (e?.message || String(e)); }
return {classifyFile, extractEntry, limitExtract, buildDocx, errorText, LIMITS};
})();
