# A static file server for the pages that tells the browser not to cache, so a
# reload always picks up edited HTML, CSS, JS and freshly built wasm.
import functools
import http.server

class NoCache(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

handler = functools.partial(NoCache, directory="public")
http.server.ThreadingHTTPServer(("", 8080), handler).serve_forever()
