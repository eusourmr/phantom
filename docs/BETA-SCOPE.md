# Phantom Engine Beta — Scope Contract

## Why this document exists

A browser can expand without limit. Phantom therefore distinguishes a bounded **Engine Beta** from a future general-purpose browser.

## Engine Beta is

A working independent Rust web-engine technology milestone that can:

- navigate HTTP/HTTPS documents through Phantom's network boundary;
- handle bounded redirect chains;
- parse a documented HTML subset into Phantom DOM;
- parse/cascade a documented CSS subset;
- perform block/inline/Flexbox layout for the supported subset;
- paint text, boxes and supported raster/animated images;
- revalidate supported cached resources;
- manage navigation/tabs in the native shell;
- fail safely on unsupported/malformed input.

## Engine Beta is not

- a Chrome/Firefox/Safari replacement;
- full HTML5 conformance;
- full CSS conformance;
- a production ECMAScript/JIT implementation;
- full accessibility support;
- a complete extension platform;
- a semantic/agent browser;
- proof that arbitrary modern sites work.

## Compatibility claim format

Every compatibility claim must name its surface. Example:

> “Phantom Engine Beta supports the project-defined HTML/CSS subset and the curated WPT corpus listed in the release notes.”

Never use “standards compliant” without a measured conformance scope.

## Resource and reliability gate

Beta requires:

- bounded document/resource sizes;
- bounded redirect depth;
- bounded cache;
- cancellation/generation guards for asynchronous resource work;
- regression tests for malformed inputs in implemented surfaces;
- all workspace tests green;
- Clippy with `-D warnings`;
- Windows native shell build green.

## What comes next

The next product milestone is a **Browser Technology Preview** with a scripting runtime boundary, DOM/event integration and a deliberately small dynamic-page corpus.
