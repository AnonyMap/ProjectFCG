// ProjectFCG's link to the browser (loaded before the app; see
// src/app/web.rs, which calls these globals). Files go through the File
// System Access API where there is one (Chrome, Edge), else uploads and
// downloads; preferences and AutoRecover go to localStorage.
let pfcgHandle = null;   // the open project's file, when the browser can save to it
let pfcgDirty = false;
window.addEventListener('beforeunload', (e) => { if (pfcgDirty) { e.preventDefault(); e.returnValue = ''; } });

// Project's function keys and Ctrl shortcuts that browsers would take
// (help, find, history, bookmarks…). The app still receives the key.
window.addEventListener('keydown', (e) => {
  const fkey = /^F([1-9]|10)$/.test(e.key) && e.key !== 'F5';
  const ctrl = (e.ctrlKey || e.metaKey) && !e.altKey && ['f', 'h', 'g', 'd', 'k', 'l', 'u', 'j', 'e', 'b', 'i'].includes(e.key.toLowerCase());
  if (fkey || ctrl) e.preventDefault();
}, true);

const PFCG_TYPES = {
  project: [{ description: 'Projects (ProjectFCG, MS Project XML)', accept: { 'application/octet-stream': ['.pfcg', '.ganttry', '.gantry', '.xml'] } }],
  pfcg: [{ description: 'ProjectFCG project', accept: { 'application/octet-stream': ['.pfcg'] } }],
  csv: [{ description: 'CSV', accept: { 'text/csv': ['.csv'] } }],
  xml: [{ description: 'MS Project XML', accept: { 'application/xml': ['.xml'] } }],
  pdf: [{ description: 'PDF', accept: { 'application/pdf': ['.pdf'] } }],
  png: [{ description: 'PNG image', accept: { 'image/png': ['.png'] } }],
};
const PFCG_ACCEPT = { project: '.pfcg,.ganttry,.gantry,.xml', csv: '.csv' };

function pfcg_can_save_in_place() { return 'showSaveFilePicker' in window; }
function pfcg_has_handle() { return pfcgHandle !== null; }
function pfcg_forget_handle() { pfcgHandle = null; }
function pfcg_set_title(t) { document.title = t; }
function pfcg_set_dirty(d) { pfcgDirty = d; }

async function pfcg_write(h, bytes) {
  const w = await h.createWritable();
  try { await w.write(bytes); await w.close(); }
  catch (e) { try { await w.abort(); } catch (_) {} throw e; }
}

function pfcg_download(name, bytes, mime) {
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  const a = document.createElement('a');
  a.href = url; a.download = name; document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60000);
}

// Resolves to {name, bytes} or null (cancelled).
async function pfcg_open(kind) {
  if ('showOpenFilePicker' in window) {
    try {
      const [h] = await window.showOpenFilePicker({ id: 'pfcg', types: PFCG_TYPES[kind] || [] });
      const f = await h.getFile();
      const bytes = new Uint8Array(await f.arrayBuffer());
      if (kind === 'project') pfcgHandle = /\.pfcg$/i.test(f.name) ? h : null;
      return { name: f.name, bytes };
    } catch (e) { if (e.name === 'AbortError') return null; throw e; }
  }
  return new Promise((resolve, reject) => {
    const i = document.createElement('input');
    i.type = 'file'; i.accept = PFCG_ACCEPT[kind] || '';
    i.onchange = async () => {
      const f = i.files[0];
      if (!f) return resolve(null);
      if (kind === 'project') pfcgHandle = null;
      try { resolve({ name: f.name, bytes: new Uint8Array(await f.arrayBuffer()) }); } catch (e) { reject(e); }
    };
    i.oncancel = () => resolve(null);
    i.click();
  });
}

// Save As / export. Resolves to the name saved under, or null (cancelled).
async function pfcg_save_as(name, data, kind, mime) {
  const bytes = data.slice(); // a copy: the view into wasm memory may not outlive this call
  if ('showSaveFilePicker' in window) {
    try {
      const h = await window.showSaveFilePicker({ id: 'pfcg-' + kind, suggestedName: name, types: PFCG_TYPES[kind] || [] });
      await pfcg_write(h, bytes);
      if (kind === 'pfcg') pfcgHandle = h;
      return h.name;
    } catch (e) { if (e.name === 'AbortError') return null; throw e; }
  }
  pfcg_download(name, bytes, mime);
  return name;
}

// Save: back to the open file. Resolves to its name, or null when there's
// no file to save to (then the app asks for one).
async function pfcg_save(data) {
  if (!pfcgHandle) return null;
  const bytes = data.slice();
  await pfcg_write(pfcgHandle, bytes);
  return pfcgHandle.name;
}

// Print: the PDF in a new tab, where the browser's viewer prints it.
function pfcg_open_pdf(data, name) {
  const url = URL.createObjectURL(new Blob([data.slice()], { type: 'application/pdf' }));
  const w = window.open(url, '_blank');
  if (!w) pfcg_download(name, data.slice(), 'application/pdf');
  setTimeout(() => URL.revokeObjectURL(url), 600000);
  return !!w;
}

function pfcg_store_get(k) { try { return localStorage.getItem(k); } catch (_) { return null; } }
function pfcg_store_set(k, v) { try { localStorage.setItem(k, v); return true; } catch (_) { return false; } }
function pfcg_store_remove(k) { try { localStorage.removeItem(k); } catch (_) {} }
