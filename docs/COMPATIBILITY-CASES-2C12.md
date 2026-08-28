# 2C-12 Compatibility Cases — WPT-ready Test Shape

The external engineering review correctly identified that browser compatibility
must eventually be measured against Web Platform Tests rather than only
hand-written demonstrations.

2C-12 does not pull the WPT repository into normal CI. Instead it starts using a
test shape designed for later WPT mapping.

`crates/phantom-engine/tests/link_navigation.rs` explicitly covers:

1. nested inline content inside `<a href>`;
2. case-insensitive `_blank`;
3. anchor without `href`;
4. empty `href`;
5. geometry regeneration after relayout.

These cases are project-owned and do not copy WPT source.

## Future harness mapping

When the curated WPT runner is introduced, each imported case should record:

- upstream WPT commit;
- upstream test path;
- Phantom feature surface;
- expected status: pass / fail / unsupported;
- any project-specific adaptation required to invoke the engine API.

This keeps ordinary CI bounded while making compatibility claims externally
measurable.
