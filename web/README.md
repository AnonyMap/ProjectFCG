# ProjectFCG in a browser (experimental)

The same app, compiled to WebAssembly. The scheduling engine is shared with the desktop build, so the web version computes the same dates. GitHub Actions builds it (`.github/workflows/web.yml`) on every push to the `web-spike` branch, and pushes the built site to the `web-build` branch.

## Trying it

The site has to be served over http(s); it won't open from `file://` because browsers refuse to load WebAssembly from local files. Any static server works:

```sh
git clone -b web-build https://github.com/AnonyMap/ProjectFCG projectfcg-web
cd projectfcg-web && python3 -m http.server 8000
# open http://localhost:8000  (or http://localhost:8000/?sample)
```

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
- Preferences are kept per browser.
- All tabs share one AutoRecover copy.
- A recovered plan has lost its link to its file, so Save asks where to save it.
