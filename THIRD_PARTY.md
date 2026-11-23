# Third-party credits

## OneClient

Polyfrost Inc. and the OneLauncher contributors develop OneClient.
Ferrocraft downloads the unmodified launcher directly from the [official OneClient 2.12.0 release](https://github.com/Polyfrost/OneLauncher/releases/tag/oneclient-2.12.0).
Ferrocraft does not redistribute the launcher, its artwork, the OneClient mod pack, or the New Default+ resource pack by SeaOfPixels.
Polyfrost's trademark terms and the resource pack license restrict redistribution of the branded release, as recorded in [issue #831](https://github.com/i098/ferrocraft/issues/831).

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

The Ferrocraft launcher uses these dependencies under the MIT license.
This list includes `cmov` and `ctutils`, which the joint launcher and engine release build enables.

- `bitflags`, `block-buffer`, `cfg-if`, `cmov`, `const-oid`, `cpufeatures`, `crypto-common`, `ctutils`, `digest`, `dirs`, `dirs-sys`, `errno`
- `fastrand`, `getrandom`, `hybrid-array`, `itoa`, `libc`, `linux-raw-sys`, `memchr`, `once_cell`
- `rustix`, `serde_core`, `serde_json`, `sha2`, `tempfile`, `typenum`, `windows-link`, `windows-registry`, `windows-result`, `windows-strings`, `windows-sys`, `zmij`

The Windows launcher also uses [`option-ext` 0.2.0](https://crates.io/api/v1/crates/option-ext/0.2.0/download) under the [Mozilla Public License 2.0](https://www.mozilla.org/MPL/2.0/).
The download contains the corresponding source and license text.

The launcher reads OneClient’s database with `rusqlite` and `libsqlite3-sys` under the MIT license below.
The bundled [SQLite](https://www.sqlite.org/copyright.html) engine is in the public domain.
The database reader also uses `fallible-iterator`, `fallible-streaming-iterator`, and `hashlink` under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).

The upstream MIT files for some of these crates, including `fastrand`, do not contain a copyright line.
The `rustix` and `linux-raw-sys` contributors retain their copyrights.

```text
Copyright (c) 2014 The Rust Project Developers (bitflags, getrandom)
Copyright (c) 2018-2025 The RustCrypto Project Developers (block-buffer)
Copyright (c) 2014 Alex Crichton (cfg-if)
Copyright (c) 2022-2026 The RustCrypto Project Developers (cmov)
Copyright (c) 2020-2026 The RustCrypto Project Developers (const-oid, cpufeatures)
Copyright (c) 2021-2026 RustCrypto Developers (crypto-common)
Copyright (c) 2025-2026 The RustCrypto Project Developers (ctutils)
Copyright (c) 2017-2025 RustCrypto Developers (digest)
Copyright (c) 2017 Artyom Pavlov (digest)
Copyright (c) 2018-2019 dirs-rs contributors (dirs, dirs-sys)
Copyright (c) 2014 Chris Wong (errno)
Copyright (c) 2018-2026 The rust-random Project Developers (getrandom)
Copyright (c) 2022-2026 The RustCrypto Project Developers (hybrid-array)
Copyright (c) The Rust Project Developers (libc)
Copyright (c) 2015 Andrew Gallant (memchr)
Copyright (c) 2014 The rusqlite developers (rusqlite, libsqlite3-sys)
Copyright (c) 2016-2026 The RustCrypto Project Developers (sha2)
Copyright (c) 2016 Artyom Pavlov (sha2)
Copyright (c) 2009-2013 Mozilla Foundation (sha2)
Copyright (c) 2006-2009 Graydon Hoare (sha2)
Copyright (c) 2015 Steven Allen (tempfile)
Copyright (c) 2014 Paho Lurie-Gregg (typenum)
Copyright (c) Microsoft Corporation. (windows-link, windows-registry, windows-result, windows-strings, windows-sys)

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