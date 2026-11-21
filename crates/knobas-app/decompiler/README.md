# The decompiler, as knobas carries it

The JAR view's *Decompile* (docs/decisions/2026-11-18-a-jar-is-drawn-as-its-contents.md)
runs Vineflower with a Java already installed on the machine. A release
installer holds the jar in this folder, staged by `scripts/fetch-vineflower.sh`
from Maven Central and checked against `vineflower.sha256`; knobas checks it
against the same pin before it runs it. knobas never downloads it at run time,
and a build without it offers no *Decompile*.

## License

Vineflower (<https://github.com/Vineflower/vineflower>) is released under the
Apache License, Version 2.0 (<https://www.apache.org/licenses/LICENSE-2.0>).
It derives from the Fernflower decompiler, Copyright 2000-2024 JetBrains s.r.o.
and contributors, under the same license. The jar is the unmodified release.
