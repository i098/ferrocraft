# Third-party credits

## OneClient

Polyfrost Inc. and the OneLauncher contributors develop OneClient.
Ferrocraft downloads the unmodified launcher directly from the [official OneClient 2.12.0 release](https://github.com/Polyfrost/OneLauncher/releases/tag/oneclient-2.12.0).
Ferrocraft does not redistribute the launcher, its artwork, the OneClient mod pack, or the New Default+ resource pack by SeaOfPixels.

- [Corresponding OneClient source](https://github.com/Polyfrost/OneLauncher/tree/oneclient-2.12.0)
- [GNU General Public License version 3](https://github.com/Polyfrost/OneLauncher/blob/oneclient-2.12.0/LICENSE)
- [Polyfrost terms and trademark notice](https://polyfrost.org/legal/terms)

OneClient and its marks belong to Polyfrost Inc.
Polyfrost does not sponsor or endorse Ferrocraft.

## Java mod platform

The Ferrocraft profile uses [Ornithe](https://ornithemc.net/).
OneClient downloads the loader when the user starts the profile.
Ferrocraft uses the [Fabric Loader API](https://github.com/FabricMC/fabric-loader/blob/0.16.10/LICENSE) under Apache License 2.0 to build its mod.
The Ferrocraft mod jar does not contain Fabric Loader.

## Launcher dependencies

The installer uses `sha2` (RustCrypto) to verify the download and `tempfile` to stage the files.
`tempfile` uses `fastrand` to generate temporary names.
The MIT license below applies to `sha2`, `tempfile`, and `fastrand`.

```text
Copyright (c) 2016-2026 The RustCrypto Project Developers (sha2)
Copyright (c) 2016 Artyom Pavlov (sha2)
Copyright (c) 2009-2013 Mozilla Foundation (sha2)
Copyright (c) 2006-2009 Graydon Hoare (sha2)
Copyright (c) 2015 Steven Allen (tempfile)

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```