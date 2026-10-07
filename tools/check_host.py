"""Checks a deployed Mira site against what Mira's output expects of a host.

    python tools/check_host.py https://example.vercel.app

Each check prints PASS, FAIL, or NOTE. FAIL means readers or agents would
see something wrong; NOTE means an optional feature the host does not do.
Exits non zero when any check fails. Standard library only.
"""

import json
import sys
import urllib.error
import urllib.request

SECURITY_HEADERS = [
    "x-content-type-options",
    "x-frame-options",
    "referrer-policy",
    "cross-origin-opener-policy",
    "permissions-policy",
    "content-security-policy",
]


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def fetch(url, accept=None, follow=True):
    request = urllib.request.Request(url, headers={"User-Agent": "mira-check-host/1", **({"Accept": accept} if accept else {})})
    opener = urllib.request.build_opener() if follow else urllib.request.build_opener(NoRedirect)
    try:
        with opener.open(request, timeout=20) as response:
            return response.status, {k.lower(): v for k, v in response.headers.items()}, response.read()
    except urllib.error.HTTPError as error:
        return error.code, {k.lower(): v for k, v in error.headers.items()}, error.read()


def main(base):
    base = base.rstrip("/")
    results = []

    def check(name, ok, detail="", optional=False):
        results.append(("PASS" if ok else ("NOTE" if optional else "FAIL"), name, detail))

    status, headers, body = fetch(base + "/")
    html = body.decode("utf-8", "replace")
    check("home page is 200 HTML", status == 200 and "text/html" in headers.get("content-type", ""), f"{status} {headers.get('content-type', '')}")
    check("page carries its hashed CSP meta tag", 'http-equiv="Content-Security-Policy"' in html)
    missing = [h for h in SECURITY_HEADERS if h not in headers]
    check("security headers sent", not missing, "missing: " + ", ".join(missing) if missing else "all present")
    hsts = "strict-transport-security" in headers
    check("HSTS sent", hsts, headers.get("strict-transport-security", "absent"), optional=not base.startswith("https"))

    # Clean URLs: a section with and without its trailing slash.
    links = [l for l in __import__("re").findall(r'href="(/[^"#?]*/)"', html) if l != "/" and not l.startswith("/_mira")]
    section = links[0] if links else None
    if section:
        status, _, _ = fetch(base + section)
        check(f"{section} serves", status == 200, str(status))
        status, headers_r, _ = fetch(base + section.rstrip("/"), follow=False)
        location = headers_r.get("location", "")
        ok = status in (301, 302, 307, 308) and location.rstrip("/").endswith(section.rstrip("/")) or status == 200
        check(f"{section.rstrip('/')} without slash resolves", ok, f"{status} {location}".strip())

    status, headers, body = fetch(base + "/this-page-does-not-exist-7q/")
    check("missing page returns 404", status == 404, str(status))
    # Every Mira page carries the runtime's config block; a host's own 404
    # page does not.
    check("404 uses the site's 404 page", b'id="mira-config"' in body, f"{len(body)} bytes")

    status, headers, body = fetch(base + "/index.md")
    check("Markdown twin served", status == 200 and body.startswith(b"---"), str(status))
    check("twin served as text/markdown", "text/markdown" in headers.get("content-type", ""), headers.get("content-type", ""))

    status, headers, body = fetch(base + "/", accept="text/markdown")
    check("Accept: text/markdown returns the twin", status == 200 and body.startswith(b"---"), headers.get("content-type", ""), optional=True)

    for path, kind in [("/llms.txt", "text/plain"), ("/robots.txt", "text/plain"), ("/_mira/search.json", "json")]:
        status, headers, _ = fetch(base + path)
        check(f"{path} served", status == 200 and kind in headers.get("content-type", ""), f"{status} {headers.get('content-type', '')}")

    status, headers, body = fetch(base + "/media.json")
    try:
        manifest = json.loads(body) if status == 200 else []
    except ValueError:
        manifest = []
        check("/media.json is JSON", False, headers.get("content-type", ""))
    if manifest:
        avif = next((f["src"] for m in manifest for f in m["formats"] if f["format"] == "avif"), None)
        if avif:
            status, headers, _ = fetch(base + avif)
            check("AVIF served as image/avif", status == 200 and headers.get("content-type", "").startswith("image/avif"), f"{status} {headers.get('content-type', '')}")
            check("media cached as immutable", "immutable" in headers.get("cache-control", ""), headers.get("cache-control", "absent"))

    width = max(len(name) for _, name, _ in results)
    for verdict, name, detail in results:
        print(f"{verdict:4}  {name.ljust(width)}  {detail}")
    failed = sum(1 for v, _, _ in results if v == "FAIL")
    print(f"\n{len(results) - failed}/{len(results)} passed" + (f", {failed} failed" if failed else ""))
    return 1 if failed else 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
