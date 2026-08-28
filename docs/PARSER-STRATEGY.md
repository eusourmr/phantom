# Phantom HTML/CSS Parser Strategy

## Current reality

Phantom already has executable HTML and CSS parsing code. Both are deliberately bounded subsets. The project must not confuse “independent implementation” with “full standards conformance”.

## HTML strategy

### Near term

Keep `phantom-html` independent and small while making its responsibilities progressively more standards-shaped:

1. tokenizer/state handling;
2. token-to-tree construction;
3. malformed-markup recovery;
4. character references;
5. raw-text/RCDATA handling;
6. documented insertion-mode subset.

The current simple stack parser is an implementation stage, not the final HTML parsing architecture.

### Compatibility oracle

Use WHATWG HTML behavior and curated Web Platform Tests as the oracle. External parser projects can be consulted for behavior and differential testing, but Phantom's production parser remains behind the `phantom-html` boundary.

### Escape hatch

If parser conformance becomes the dominant schedule risk, the project may evaluate an external standards parser behind an adapter **without changing Phantom's DOM/layout/network architecture**. Such a decision requires an ADR and is not equivalent to forking a browser engine.

## CSS strategy

The CSS layer expands from observed compatibility failures, not property-count ambition.

Priorities:

1. tokenizer/error recovery;
2. selector correctness;
3. cascade/specificity/inheritance;
4. computed values;
5. properties required by layout/paint milestones.

Unknown or unsupported declarations must fail locally rather than corrupt the entire stylesheet.

## Testing strategy

For both parsers:

- unit tests for tokenizer/parser states;
- regression tests for every fixed malformed-input bug;
- curated WPT cases;
- differential tests where useful;
- fuzzing before broad untrusted-web claims;
- input-size/time/resource bounds.

## Non-goal

Do not attempt “all HTML and all CSS” before Engine Beta. Engine Beta ships a named compatibility subset with measured tests.
