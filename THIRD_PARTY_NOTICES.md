# Third-party notices

NSFDeck compiles the emulation core of NSFPlay (`vendor/nsfplay/xgm` and `vendor/nsfplay/vcm`, a git
submodule). That code and the components it bundles are distributed under the terms below.

## NSFPlay

https://github.com/bbbradsmith/nsfplay. Maintained by Brad Smith, a fork of NSFPlay/NSFplug by Brezza
(Digital Sound Antiques). From its `readme.txt`:

> I have presumed based on text comments and readme files in the original code that it is distributed
> freely, and modification and redistribution is permitted. The same permissive license applies to this
> version of the code I maintain. You may reuse this code without restriction, and no warranty or
> liability is implied on my part.

From the original XGM source archive (`xgm/readme.txt`):

> This source archive is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY. You
> can reuse these source code freely.

## KM6502

6502 CPU emulator by Mamiya (`xgm/devices/CPU/km6502`). Public domain.

## emu2413, emu2149, emu2212

YM2413/VRC7, YM2149/5B and SCC emulators by Mitsutaka Okazaki
(https://github.com/digital-sound-antiques). emu2413 is distributed under the MIT License:

> Copyright (C) 2020 Mitsutaka Okazaki
>
> Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
> associated documentation files (the "Software"), to deal in the Software without restriction,
> including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense,
> and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so,
> subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or substantial
> portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT
> LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN
> NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
> WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
> SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

Rust crates and npm packages used by NSFDeck carry their own licenses, listed in their package metadata.
