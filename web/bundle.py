#!/usr/bin/env python3
"""Make the single-file web build: ProjectFCG.html with the app inside.

    python3 web/bundle.py OUT_DIR

OUT_DIR holds wasm-bindgen's `--target no-modules` output (projectfcg.js
and projectfcg_bg.wasm). Writes OUT_DIR/ProjectFCG.html, which runs when
opened straight from disk: plain scripts only (browsers don't load
JavaScript modules or fetch files from file://), and the app's WebAssembly
embedded, gzipped and base64-encoded, unpacked by the browser on load.

The page carries a Content Security Policy that forbids it any network
access: no requests (fetch, XHR, WebSocket, beacons), no images, fonts,
styles or scripts from anywhere else, no forms, frames or workers. Only
its own three scripts may run (each allowed by its SHA-256 hash), and they
may compile the app's WebAssembly. So nothing typed into the app can
leave the computer from the page itself; files leave only through the
Save and Export dialogs the person uses.

    python3 web/bundle.py --csp PAGE.html   (adds the policy to a built page)
"""
import base64, gzip, hashlib, pathlib, re, sys

web = pathlib.Path(__file__).parent
root = web.parent


def script(text):
    # nothing inside a <script> may close it
    return text.replace("</script", "<\\/script")


# Inline scripts that run (not the data block, which has a type).
RUNNABLE = re.compile(r"<script>(.*?)</script>", re.S)


def policy(page):
    hashes = " ".join(
        "'sha256-" + base64.b64encode(hashlib.sha256(body.encode("utf-8")).digest()).decode() + "'"
        for body in RUNNABLE.findall(page)
    )
    return "; ".join(
        [
            "default-src 'none'",
            f"script-src {hashes} 'wasm-unsafe-eval'",
            # egui positions its text input with inline styles
            "style-src 'unsafe-inline'",
            # the icon; Copy Picture and Print make blob: images and PDFs
            "img-src data: blob:",
            # the PDF opened for printing shows in the browser's viewer
            "object-src blob:",
            "connect-src 'none'",
            "font-src 'none'",
            "media-src 'none'",
            "frame-src 'none'",
            "worker-src 'none'",
            "manifest-src 'none'",
            "form-action 'none'",
            "base-uri 'none'",
        ]
    )


def add_csp(page):
    """The page with its Content Security Policy (computed last, from the
    scripts as they are)."""
    meta = '<meta http-equiv="Content-Security-Policy" content="' + policy(page) + '">'
    if "{{CSP}}" in page:
        return page.replace("{{CSP}}", meta)
    return page.replace('<meta charset="utf-8">', '<meta charset="utf-8">\n  ' + meta, 1)


def build(out):
    version = re.search(r'^version = "(.+)"', (root / "Cargo.toml").read_text(), re.M).group(1)
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
    page = add_csp(page)
    (out / "ProjectFCG.html").write_text(page)
    print(f"ProjectFCG.html: {len(page) / 1e6:.1f} MB (WebAssembly {len(wasm) / 1e6:.1f} MB, gzipped {len(gz) / 1e6:.1f} MB)")


if __name__ == "__main__":
    if sys.argv[1] == "--csp":
        p = pathlib.Path(sys.argv[2])
        p.write_text(add_csp(p.read_text()))
    else:
        build(pathlib.Path(sys.argv[1]))
