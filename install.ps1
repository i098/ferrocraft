# Install ferrocraft: download the release archive for Windows and run its installer.
# Needs `gh` (logged in) while the repo is private; falls back to Invoke-WebRequest once it is public.
& {
  $ErrorActionPreference = 'Stop'
  $repo = 'i098/ferrocraft'
  $asset = 'ferrocraft-x86_64-pc-windows-msvc.tar.gz'
  $tmp = Join-Path ([IO.Path]::GetTempPath()) ("ferrocraft-" + [guid]::NewGuid())
  New-Item -ItemType Directory $tmp | Out-Null
  try {
    $useGh = $false
    if (Get-Command gh -ErrorAction SilentlyContinue) {
      $ErrorActionPreference = 'Continue'
      gh auth status *> $null
      $useGh = $LASTEXITCODE -eq 0
      $ErrorActionPreference = 'Stop'
    }
    if ($useGh) {
      gh release download --repo $repo --pattern $asset --dir $tmp
      if ($LASTEXITCODE -ne 0) { throw "gh release download failed" }
    } else {
      Invoke-WebRequest "https://github.com/$repo/releases/latest/download/$asset" -OutFile (Join-Path $tmp $asset)
    }
    tar -xzf (Join-Path $tmp $asset) -C $tmp
    if ($LASTEXITCODE -ne 0) { throw "tar failed" }
    & (Join-Path $tmp 'ferrocraft-launcher.exe') install
    if ($LASTEXITCODE -ne 0) { throw "installer failed" }
  } finally {
    Remove-Item -Recurse -Force $tmp
  }
}
