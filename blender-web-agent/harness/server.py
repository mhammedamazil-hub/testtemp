import http.server, os
BIN = "/home/user/bw/blender-web-local/bin"
AGENT = "/home/user/bw-probe/agent"
MIME = {".wasm":"application/wasm",".js":"text/javascript",".html":"text/html",".py":"text/plain",
        ".data":"application/octet-stream",".json":"application/json"}
class H(http.server.SimpleHTTPRequestHandler):
    def translate_path(self, path):
        p = path.split("?")[0].split("#")[0]
        if p.startswith("/bin/"): return os.path.join(BIN, p[5:])
        if p in ("/", ""): return os.path.join(AGENT, "agent.html")
        return os.path.join(AGENT, p.lstrip("/"))
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy","same-origin")
        self.send_header("Cross-Origin-Embedder-Policy","require-corp")
        self.send_header("Cross-Origin-Resource-Policy","same-origin")
        self.send_header("Cache-Control","no-store")
        super().end_headers()
    def guess_type(self, path):
        return MIME.get(os.path.splitext(path)[1].lower(), "application/octet-stream")
    def log_message(self,*a): pass
http.server.ThreadingHTTPServer(("0.0.0.0",8090),H).serve_forever()
