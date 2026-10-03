# hwtr

A native PC port of Hot Wheels Turbo Racing for the PlayStation (Stainless
Games / Electronic Arts, 1999, `SLUS-00964`), written in Rust from a reverse
engineering of the game. It plays your own disc.

**Status: reverse engineering has started; nothing plays yet.**

## Not an emulator

The port will run none of the PlayStation's code. The game is rewritten in
Rust, subsystem by subsystem, and each part is checked against the original
function: the original runs instruction by instruction in a small R3000A + GTE
interpreter written for this project, and the port has to produce the same
results. During play nothing from the PlayStation runs and no BIOS is needed.

## What is here

| path | what |
| --- | --- |
| `crates/hwtr-disc` | reads the CD: CUE sheets, raw Mode 2 sectors, ISO 9660 with CD-XA, CD-DA tracks |
| `crates/hwtr-data` | reads the game's data: the BIG archives, and the asset formats as they are decoded |
| `crates/hwtr-psx` | the PS-X EXE format, an R3000A + GTE disassembler, static analysis |
| `crates/hwtr-re` | the reverse engineering command line |
| `symbols/` | names for the executable's functions and data, with confidence notes |
| `docs/` | the [reference](docs/README.md): what is true about the disc, formats and engine |
| `docs/cut/` | cut, missing and unreachable content, one page each |
| `WORKLOG.md` | the [worklog](WORKLOG.md): how each fact was found |
| `plans/` | what comes next |

The reverse engineering tools are Rust, in the same workspace as the port, so
the code that proved a format is the code the port uses.

## Using it

Put the disc where the tools look for it, then extract:

```sh
mkdir -p work/disc && 7z x -owork/disc "original/Hot Wheels - Turbo Racing.7z"
cargo run -q -p hwtr-re -- disc info              # tracks, volume, sector kinds
cargo run -q -p hwtr-re -- disc extract           # files -> work/fs
cargo run -q -p hwtr-re -- disc audio             # the 13 music tracks -> work/audio
cargo run -q -p hwtr-re -- big ls work/fs/CCCPSX.BIG
cargo run -q -p hwtr-re -- big extract work/fs/CCCPSX.BIG    # -> work/big
cargo run -q -p hwtr-re -- funcs work/fs/CCCPSX.EXE
cargo run -q -p hwtr-re -- disasm work/fs/CCCPSX.EXE --from 0x80010a5c --count 64
cargo run -q -p hwtr-re -- strings work/fs/CCCPSX.EXE
```

Any file argument may be `disc:NAME` to read straight off the CD.

No game data is in this repository; `original/` and `work/` are ignored.

## License

MIT or Apache-2.0, at your option. Hot Wheels is a trademark of Mattel; this
project is not affiliated with Mattel, Electronic Arts or Stainless Games.
