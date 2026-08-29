# Phantom 2D-5 — Security Regression Matrix

| Finding | Control | Regression test |
|---|---|---|
| SG-001 gzip expansion | decoded output limit | `gzip_expansion_is_bounded_after_decompression` |
| SG-001 brotli expansion | decoded output limit | `brotli_expansion_is_bounded_after_decompression` |
| SG-001 document gzip expansion | decoded output limit on document path | `gzip_document_expansion_is_bounded_after_decompression` |
| SG-002 literal loopback | pre-transport policy | `public_document_blocks_loopback_subresource_before_transport` |
| SG-002 explicit local use | private top-level exception | `explicit_private_top_level_can_load_its_own_loopback_resource` |
| SG-002 DNS private address | filtered connector resolver | unit IP classifier + resolver implementation |
| SG-002 public -> private redirect | redirect target policy + filtered resolver | net unit policy test |
| SG-003 mixed content | HTTPS -> HTTP deny | `secure_document_blocks_http_subresource_before_transport` |
| SG-004 favicon fanout | candidate cap | browser resource-security suite |
| SG-004 preload fanout | preload cap | browser resource-security suite |
| SG-004 duplicate preload/image | URL merge | browser resource-security suite |
| SG-004 total fetches | shared document fetch budget | browser resource-security suite |
| SG-004 body consumption | shared byte reservations | browser resource-security suite |
| SG-004 stale favicon work | cancellation token | browser resource-security suite |
