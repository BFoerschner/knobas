# 2026-11-18: *Decompile* finds the Javas IntelliJ downloads, and the first new enough

**Found in the ux19 Windows pass** (Björn's PC, v3.2.9-ux19 installed by `msiexec /a` into a scratch folder). The JAR
view offers *Decompile* only where knobas finds a Java 17 or newer (`2026-11-18-a-jar-is-drawn-as-its-contents.md`).
On Björn's PC there is no `java` in `PATH`, no `JAVA_HOME` and nothing in Program Files: his two Javas are the ones
IntelliJ IDEA downloaded, `C:\Users\bjfoer\.jdks\temurin-24.0.2` and `C:\Users\bjfoer\.jdks\temurin-25` (and the
runtime IntelliJ runs on, under its own install). knobas looked in Program Files' vendor folders on Windows and in
SDKMAN!'s, mise's and macOS's folders elsewhere, never in `~/.jdks`, so for him the JAR view would show outlines only
and Settings › Renderers › *Decompiler* would say *No Java found on this machine*. Named in Settings by hand, Temurin
25 decompiles `LedgerApplication.class` in 642 ms cold and 10 ms from the cache in knobas's own window.

The search also took the first `java` that answered, too old or not: a Java 8 first in `PATH` hid a Java 21 in an
install folder behind *Java 8 found; Decompile needs Java 17*.

## Taken

- **`~/.jdks` is looked in on every platform** (`home_roots`, `%USERPROFILE%` on Windows, `$HOME` elsewhere), after
  `PATH` and before the machine-wide folders, the newest-looking name first, as the other folders are.
- **The first Java new enough is taken** (`first_java`): the candidates are tried in order until one meets the
  bundled decompiler's floor; with none new enough, the first Java found is named, so the view can say why not.
- **A Java set in Settings is used as it is**, too old or not: the person picked it, and the reason shows beside it.

## Not taken

- **IntelliJ's own runtime** (`IntelliJ IDEA …\jbr`): it is the IDE's, moves with its updates, and a person who has
  IntelliJ has had it offer to download a JDK into `~/.jdks`.
- **Scoop's and Chocolatey's folders**: their layouts (`scoop\apps\<name>\current`) need their own walk; none is on the
  machines measured.

## Tests

`a_java_intellij_downloaded_is_found_under_the_home` (two Java homes and a folder that is none under a home's `.jdks`:
the two, the newer first) and `the_first_java_new_enough_is_taken` (fake `java`s answering 11, 21 and 25: 21 with a
floor of 17; 11 when it is the only one; the first with no floor), in `knobas-app`'s `archive::java`.
