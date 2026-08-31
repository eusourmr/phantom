# Phantom 2D-2 — URL/Origin Domain Consolidation

## Typed origin

`phantom-net` now owns a public `Origin` domain type and `OriginScheme`.

Origin identity is the normalized tuple:
- scheme;
- canonical host;
- effective port.

Default HTTP/HTTPS ports are normalized through `url::Url`; Phantom does not
reimplement host, IDNA or origin serialization.

`HttpUrl::origin()` and `HttpUrl::same_origin()` are the policy APIs.

## NetworkIsolationKey

The isolation key now stores typed `Origin` values instead of serialized
strings. Compatibility string accessors remain, while new typed accessors are
available for security policy.

## Document URL contract

`DocumentResponse` keeps typed requested and final `HttpUrl` values after
admission. Existing string accessors remain for compatibility. The browser uses
the typed final URL at document commit.
