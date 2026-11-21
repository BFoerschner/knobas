# A search fits the Windows main thread (2026-11-19)

The ux21 Windows pass, PR #2479.

## What happened

On Björn's PC, v3.2.9-ux21 ended the moment the launcher was asked anything: the first letter typed, or
the `search` command called with `p`, killed `knobas-app.exe` with exception `0xc00000fd`, a stack
overflow, always at the same offset. It did so on the upgraded scale profile and on an empty fresh one,
so not the data. v3.2.9-ux20 answered the same call. PostgreSQL was left running each time.

## Why

The fault is in `__chkstk`, the stack probe a function runs when it asks for a large frame. The shipped
executable's largest frames were, in ux20, 146 kB; in ux21, 444 kB (twice), 226 kB (twice) and 222 kB.
The 444 and 226 kB frames are Tauri's async-command path (`tauri/src/ipc/mod.rs`, which builds the
command's future and spawns it), and the 222 kB one is tokio's `spawn`. On Windows that path runs on the
main thread, whose stack is 1 MB (`SizeOfStackReserve` 0x100000), and each of those frames holds
several copies of the command's future (the MSVC target appears to reuse few stack slots across them).

The fuzzy search made the future of `Searcher::search` 37,128 bytes (ux20: 10,344): `search_as` joined
four branches inline, three of them holding a whole `ask` (both statement paths). Docs' `page_search`
grew to 18,424 bytes the same way. The frames grew in step (3x), and the main thread ran out.

On Linux and the Mac the same search needs 128 to 256 kB of stack in all, and the main thread there has
8 MB, which is why no gate on the Mac saw it.

## What changed

Each branch of the two joins is boxed (`Box::pin`), so a command's future holds pointers to them: the
launcher's search future is 2,464 bytes, the Docs search's 9,840, and a search polls in 64 to 128 kB of
stack. `knobas_search::FUTURE_MOST` (16 kB) is the line, held by `tests/it/future_size.rs`, red on
ux21's code. A branch that grows a search's future past it goes on the heap too.

## Owed

The fix is measured on x86-64 Linux (the WSL box); the crash needs a Windows build to be shown gone: the
next pre-release's boot check calls `search` and types into the launcher.
