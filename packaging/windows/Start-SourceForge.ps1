# SourceForge portable launcher for Windows.
#
# Serves the bundled static app on http://127.0.0.1:<port> with a built-in
# .NET HttpListener (no Node, Python or installer required) and opens the
# default browser. Data stays in the browser profile.
#
# Verified: static analysis only (this build environment has no Windows host
# or PowerShell). Treat it as unverified on Windows until CI runs it.
param(
  [int]$Port = 8787,
  [switch]$NoBrowser
)

$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot 'app'
if (-not (Test-Path (Join-Path $root 'index.html'))) {
  Write-Error "SourceForge app files not found in $root"
  exit 1
}

$mime = @{
  '.html' = 'text/html; charset=utf-8'
  '.js'   = 'text/javascript; charset=utf-8'
  '.mjs'  = 'text/javascript; charset=utf-8'
  '.css'  = 'text/css; charset=utf-8'
  '.json' = 'application/json; charset=utf-8'
  '.webmanifest' = 'application/manifest+json; charset=utf-8'
  '.svg'  = 'image/svg+xml'
  '.png'  = 'image/png'
  '.jpg'  = 'image/jpeg'
  '.ico'  = 'image/x-icon'
  '.wasm' = 'application/wasm'
  '.woff2' = 'font/woff2'
  '.txt'  = 'text/plain; charset=utf-8'
  '.map'  = 'application/json; charset=utf-8'
}

$listener = [System.Net.HttpListener]::new()
$prefix = "http://127.0.0.1:$Port/"
$listener.Prefixes.Add($prefix)
try {
  $listener.Start()
} catch {
  Write-Error "Could not listen on $prefix — is another SourceForge window running? ($($_.Exception.Message))"
  exit 1
}

$url = $prefix
Write-Host "SourceForge is running at $url"
Write-Host "Keep this window open. Press Ctrl+C to stop."
if (-not $NoBrowser) { Start-Process $url }

try {
  while ($listener.IsListening) {
    $context = $listener.GetContext()
    $request = $context.Request
    $response = $context.Response
    try {
      $relative = [System.Uri]::UnescapeDataString($request.Url.AbsolutePath.TrimStart('/'))
      if ([string]::IsNullOrWhiteSpace($relative)) { $relative = 'index.html' }
      $candidate = Join-Path $root $relative
      $resolved = [System.IO.Path]::GetFullPath($candidate)
      $rootFull = [System.IO.Path]::GetFullPath($root)
      if (-not $resolved.StartsWith($rootFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        $response.StatusCode = 403
        $response.Close()
        continue
      }
      if (-not (Test-Path $resolved -PathType Leaf)) {
        # SPA fallback so deep links and manifest shortcuts still load the shell.
        $resolved = Join-Path $root 'index.html'
      }
      $bytes = [System.IO.File]::ReadAllBytes($resolved)
      $ext = [System.IO.Path]::GetExtension($resolved).ToLowerInvariant()
      $response.ContentType = if ($mime.ContainsKey($ext)) { $mime[$ext] } else { 'application/octet-stream' }
      $response.Headers.Add('Cache-Control', 'no-cache')
      $response.ContentLength64 = $bytes.Length
      $response.OutputStream.Write($bytes, 0, $bytes.Length)
    } catch {
      try { $response.StatusCode = 500 } catch {}
    } finally {
      try { $response.Close() } catch {}
    }
  }
} finally {
  $listener.Stop()
  $listener.Close()
}
