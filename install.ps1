# Install ferrocraft: download the release archive for Windows and run its installer.
& {
  $ErrorActionPreference = 'Stop'
  $repo = 'i098/ferrocraft'
  $asset = 'ferrocraft-x86_64-pc-windows-msvc.tar.gz'
  $tmp = Join-Path ([IO.Path]::GetTempPath()) ("ferrocraft-" + [guid]::NewGuid())
  New-Item -ItemType Directory $tmp | Out-Null
  try {
    Invoke-WebRequest "https://github.com/$repo/releases/latest/download/$asset" -OutFile (Join-Path $tmp $asset)
    tar -xzf (Join-Path $tmp $asset) -C $tmp
    if ($LASTEXITCODE -ne 0) { throw "tar failed" }
    & (Join-Path $tmp 'ferrocraft-launcher.exe') install
    if ($LASTEXITCODE -ne 0) { throw "installer failed" }
  } finally {
    Remove-Item -Recurse -Force $tmp
  }
}
