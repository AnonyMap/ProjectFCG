# ProjectFCG in a browser (experimental)

The same app, compiled to WebAssembly and packed into one HTML file. The scheduling engine is shared with the desktop build, so the web version computes the same dates. Every release carries it as `ProjectFCG-vX.Y.Z-web.html`. GitHub Actions also builds it on every push to `main` (`.github/workflows/web.yml`) and keeps the latest on the `web-build` branch.

## Running it

The build is **one file, `ProjectFCG.html`** (about 5 MB), with the whole app inside. **Double-click it**, or drag it into a Chrome or Edge window. It needs no web server, no install and no network connection: it runs entirely from disk.

Get it from a release, or the latest build from the `web-build` branch (`https://github.com/AnonyMap/ProjectFCG/raw/web-build/ProjectFCG.html`). Add `?sample` to the address to open the sample plan.

How a single file can work: browsers won't load JavaScript modules or fetch other files from `file://`. So `web/bundle.py` builds the page from plain scripts only:
- the browser shim, `web/pfcg.js`;
- wasm-bindgen's `no-modules` loader;
- the app's WebAssembly, gzipped and base64-encoded, which the browser unpacks with its built-in `DecompressionStream`.

A page opened from disk counts as a secure context in Chrome and Edge, so their file dialogs and save-in-place work there just as on a website.

The same file is also published as `index.html`, ready to host (GitHub Pages, say) if a link is ever wanted.

## Files and saving

- **In Chrome and Edge**, Open and Save As use the system file dialogs. Save writes back to the same file, and the first save to a file asks permission once. The browser writes to a temporary file and swaps it in when complete, so a failed save leaves the old file intact.
- **AutoSave** (on by default in the browser) then saves into the file a few seconds after each change, with no dialog. It starts after the plan's first Save or Save As in the tab, because browsers only ask for permission to change a file in answer to a click. If a background save fails (the file was moved, say), AutoSave pauses until the next Save.
- **In Firefox and Safari**, Open uploads a file and Save downloads a copy, every time. These browsers don't have the File System Access API, so a page can't write into a file on disk, and that includes the HTML file rewriting itself. AutoRecover still keeps unsaved work in the browser. Brave has the API behind a flag: `brave://flags/#file-system-access-api`.
- **AutoRecover** keeps a copy of unsaved work in the browser's storage 10 seconds after the last change. If the tab closes without saving, the next visit offers to recover it.
- **Closing the tab** with unsaved changes makes the browser ask first.
- **Print** opens the PDF in a new tab, where the browser prints it.
- **Exports** (CSV, XML, PDF, Copy Picture) save through the same dialogs or as downloads.

## Differences from the desktop app

- Ctrl+N, Ctrl+W and Ctrl+T belong to the browser and can't be overridden: use File ▸ New. The page claims Project's other keys from the browser: F1–F10 and F12, Ctrl+F/H/G/D/−/=/0, and Alt+arrows/Home/End.
- File ▸ Exit is hidden, because a page can't close its own tab.
- An internal error ends the page instead of being recovered in place. AutoRecover is the safety net.
- Leveling runs in the foreground, so very large plans pause the page while they level.
- Preferences are kept per browser. Chrome gives every file opened from disk the same storage, so all copies of `ProjectFCG.html` on a computer share their preferences and AutoRecover copy.
- All tabs share one AutoRecover copy.
- A recovered plan has lost its link to its file, so Save asks where to save it.
