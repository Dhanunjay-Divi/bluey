# Bluey Jobs Automation Third-Party Notices

This package includes runtime document tooling and bundled fonts in addition
to the dependencies recorded by the package lock.

## @pdf-lib/fontkit 1.1.1

License: MIT

Author: Andrew Dillon

Contributor and original fontkit author: Devon Govett

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## pdfjs-dist 6.2.108

License: Apache License 2.0

The installed dependency distributes the complete Apache 2.0 license in its
own `LICENSE` file. Bluey uses PDF.js only for bounded PDF parsing and text-layer
validation before an application document can enter a submission packet.

## parse5 7.3.0

License: MIT

Copyright (c) 2013-2019 Ivan Nikulin

Bluey uses parse5 to read bounded, allowlisted HTML job tables without
executing third-party scripts. The complete license is distributed by the
installed dependency and retained in the package-manager artifact.

## entities 6.0.1

License: BSD-2-Clause

Copyright (c) Felix Bohm

This transitive parse5 dependency decodes HTML entities while reading curated
job tables. The complete license is distributed by the installed dependency
and retained in the package-manager artifact.

## Bundled Noto Fonts

`assets/fonts/NotoSans-Regular.ttf`, `NotoSansSC-Regular.ttf`, and
`NotoSansKR-Regular.ttf` are licensed under the SIL Open Font License 1.1. The
complete license texts accompany them as `Noto-LICENSE.txt` and
`NotoCJK-LICENSE.txt`. Source revisions, transformations, and SHA-256 hashes
are recorded in the repository-level `THIRD_PARTY_PROVENANCE.md`.
