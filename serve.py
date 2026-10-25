import os
import re
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

class RangeHTTPRequestHandler(SimpleHTTPRequestHandler):
    def send_head(self):
        path = self.translate_path(self.path)
        if os.path.isdir(path):
            return super().send_head()
        ctype = self.guess_type(path)
        try:
            f = open(path, 'rb')
        except OSError:
            self.send_error(404, "File not found")
            return None

        fs = os.fstat(f.fileno())
        size = fs.st_size
        range_header = self.headers.get('Range')
        if range_header:
            m = re.match(r'bytes=(\d+)-(\d*)', range_header)
            if m:
                start = int(m.group(1))
                end = int(m.group(2)) if m.group(2) else size - 1
                if start >= size:
                    f.close()
                    self.send_error(416, "Requested Range Not Satisfiable")
                    return None
                end = min(end, size - 1)
                length = end - start + 1
                self.send_response(206)
                self.send_header("Content-type", ctype)
                self.send_header("Accept-Ranges", "bytes")
                self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
                self.send_header("Content-Length", str(length))
                self.send_header("Access-Control-Allow-Origin", "*")
                self.end_headers()
                f.seek(start)
                self._range_bytes_left = length
                return f

        self._range_bytes_left = None
        self.send_response(200)
        self.send_header("Content-type", ctype)
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(size))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        return f

    def copyfile(self, source, outputfile):
        if getattr(self, '_range_bytes_left', None) is not None:
            left = self._range_bytes_left
            bufsize = 64 * 1024
            while left > 0:
                chunk = source.read(min(bufsize, left))
                if not chunk:
                    break
                outputfile.write(chunk)
                left -= len(chunk)
        else:
            super().copyfile(source, outputfile)

if __name__ == '__main__':
    ThreadingHTTPServer.allow_reuse_address = True
    server = ThreadingHTTPServer(('0.0.0.0', 8000), RangeHTTPRequestHandler)
    print("Serving on http://0.0.0.0:8000", flush=True)
    server.serve_forever()
