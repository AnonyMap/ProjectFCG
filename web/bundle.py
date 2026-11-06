#!/usr/bin/env python3
"""Make the single-file web build: ProjectFCG.html with the app inside.

    python3 web/bundle.py OUT_DIR

OUT_DIR holds wasm-bindgen's `--target no-modules` output (projectfcg.js
and projectfcg_bg.wasm). Writes OUT_DIR/ProjectFCG.html, which runs when
opened straight from disk: plain scripts only (browsers don't load
JavaScript modules or fetch files from file://), and the app's WebAssembly
embedded, gzipped and base64-encoded, unpacked by the browser on load.
"""
import base64, gzip, pathlib, re, sys

out = pathlib.Path(sys.argv[1])
web = pathlib.Path(__file__).parent
root = web.parent
version = re.search(r'^version = "(.+)"', (root / "Cargo.toml").read_text(), re.M).group(1)


def script(text):
    # nothing inside a <script> may close it
    return text.replace("</script", "<\\/script")


wasm = (out / "projectfcg_bg.wasm").read_bytes()
gz = gzip.compress(wasm, compresslevel=9, mtime=0)
page = (web / "template.html").read_text()
for key, value in {
    "VERSION": version,
    "ICON": base64.b64encode((root / "packaging" / "projectfcg.svg").read_bytes()).decode(),
    "PFCG_JS": script((web / "pfcg.js").read_text()),
    "GLUE_JS": script((out / "projectfcg.js").read_text()),
    "WASM": base64.b64encode(gz).decode(),
}.items():
    page = page.replace("{{" + key + "}}", value)
(out / "ProjectFCG.html").write_text(page)
print(f"ProjectFCG.html: {len(page) / 1e6:.1f} MB (WebAssembly {len(wasm) / 1e6:.1f} MB, gzipped {len(gz) / 1e6:.1f} MB)")
