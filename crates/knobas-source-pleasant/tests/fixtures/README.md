# Pleasant answers the adapter's tests replay

`tree.json` is a **stand-in**, written from the vendor's documented shapes
(keepasshub.com, REST API v5: Folders, Entries) before the real free server
was recorded: the clickdummy's company, one entry per code-key format, a
comment-required folder, a field the listing sends and one it does not. When
the server agent's recordings land in `S/pleasant-fixtures/`, they replace
these files, and every test here runs against them unchanged.
