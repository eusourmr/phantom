# Phantom — Continuidade após 2C-13

## 2C status

The browser interaction chain now includes:

- typed/manual URL navigation;
- bounded redirects;
- document cache and reload revalidation;
- site identity;
- clickable hyperlinks;
- editable first-slice form controls;
- GET form submission.

This materially advances the goal of making the pre-script browser useful
before broadening standards coverage.

## Feedback alignment

2C-13 reinforces prior architectural decisions:

- URL parsing/encoding stays on rust-url;
- DOM remains separate from core;
- current form state does not force mutable DOM yet;
- no Rc<RefCell> tree is introduced;
- no scripting engine is added prematurely;
- external compatibility is approached with explicit test cases;
- no speculative GPU/QUIC/tracing dependency is introduced.

## Next milestone

### 2C-14 — Fragment Navigation + Navigation Lifecycle II

Engine/browser:
- same-document `#fragment` recognition;
- target lookup by `id`;
- scroll-to-target;
- correct history entries for fragment navigation;
- Back/Forward restoration across fragment positions;
- link activation avoids unnecessary document refetch when only fragment changes.

Visual/browser increment:
- lightweight active-navigation/scroll restoration feedback without adding
  another permanent toolbar.

2C-15 can then close the 2C line with Site Identity II and navigation polish.
