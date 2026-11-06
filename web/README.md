# ProjectFCG in a browser (experimental)

The same app, compiled to WebAssembly and packed into one HTML file. The scheduling engine is shared with the desktop build, so the web version computes the same dates. GitHub Actions builds it (`.github/workflows/web.yml`) on every push to the `web-spike` branch, and pushes the built site to the `web-build` branch.

## Running it

The build is **one file, `ProjectFCG.html`** (about 5 MB), with the whole app inside. **Double-click it**, or drag it into a Chrome or Edge window. It needs no web server, no install and no network connection: it runs entirely from disk.

Get it from the `web-build` branch (`https://github.com/AnonyMap/ProjectFCG/raw/web-build/ProjectFCG.html`) or from the latest run of *Actions ▸ Web build*. Add `?sample` to the address to open the sample plan.

How a single file can work: browsers won't load JavaScript modules or fetch other files from `file://`. So `web/bundle.py` builds the page from plain scripts only:
- the browser shim, `web/pfcg.js`;
- wasm-bindgen's `no-modules` loader;
- the app's WebAssembly, gzipped and base64-encoded, which the browser unpacks with its built-in `DecompressionStream`.

A page opened from disk counts as a secure context in Chrome and Edge, so their file dialogs and save-in-place work there just as on a website.

The same file is also published as `index.html`, ready to host (GitHub Pages, say) if a link is ever wanted.

## Files and saving

- **In Chrome and Edge**, Open and Save As use the system file dialogs. Save writes back to the same file, and the first save to a file asks permission once. The browser writes to a temporary file and swaps it in when complete, so a failed save leaves the old file intact.
- **In Firefox and Safari**, Open uploads a file and Save downloads a copy.
- **AutoRecover** keeps a copy of unsaved work in the browser's storage 10 seconds after the last change. If the tab closes without saving, the next visit offers to recover it.
- **Closing the tab** with unsaved changes makes the browser ask first.
- **Print** opens the PDF in a new tab, where the browser prints it.
- **Exports** (CSV, XML, PDF, Copy Picture) save through the same dialogs or as downloads.

## Differences from the desktop app

- Some shortcuts belong to the browser and can't be overridden: Ctrl+N, Ctrl+W and Ctrl+T always open or close windows and tabs. Use the File menu for New.
- An internal error ends the page instead of being recovered in place. AutoRecover is the safety net.
- Leveling runs in the foreground, so very large plans pause the page while they level.
- Preferences are kept per browser. Chrome gives every file opened from disk the same storage, so all copies of `ProjectFCG.html` on a computer share their preferences and AutoRecover copy.
- All tabs share one AutoRecover copy.
- A recovered plan has lost its link to its file, so Save asks where to save it.
