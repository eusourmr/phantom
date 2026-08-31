# Phantom 2C-14 — Navigation Lifecycle II

Same-document fragment navigation does not spawn a network worker or increment
the document generation.

Each history entry has a scroll offset. Back/Forward restores its saved
position. Reload remains a real reload/revalidation and preserves current scroll
intent.

Initial `page#fragment` navigation preserves the requested fragment when the
network final URL does not provide one, then scrolls after layout is available.

No permanent toolbar is added; the existing status surface gives lightweight
fragment feedback.
