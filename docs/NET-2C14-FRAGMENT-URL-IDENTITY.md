# Phantom 2C-14 — Fragment URL Identity

`HttpUrl` now owns `fragment`, `with_fragment`, `without_fragment` and
`same_document_except_fragment`.

Only the fragment is ignored for same-document identity. Query, path, scheme,
host and port remain significant.
