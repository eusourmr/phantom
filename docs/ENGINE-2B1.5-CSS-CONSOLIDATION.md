# Phantom Engine 2B-1.5 — CSS Core Consolidation

## Objective

Freeze a stronger style-system contract before Paint v2 starts consuming layout.

The CSS stage remains fully owned by Phantom.

## Pipeline wall

HTML
  -> DOM snapshot
  -> CSS parser / selector matcher / cascade
  -> interned ComputedStyle snapshot
  -> LayoutSnapshot
  -> Paint (next)

Layout receives typed computed values. It does not parse selectors, declarations,
or CSS text.

## What is consolidated

### Parser

The parser now scans rule blocks instead of splitting blindly on `}`.
It tracks:
- strings;
- escapes;
- nested braces;
- parentheses in declarations;
- comments.

Malformed or unsupported rules are isolated instead of invalidating the page.

### Selectors

Current supported subset:
- `*`
- `tag`
- `.class`
- `#id`
- compound selectors such as `div.card#hero`
- descendant combinator: `main .card`
- child combinator: `main > .card`
- comma-separated selector groups

Deferred:
- pseudo-classes;
- pseudo-elements;
- attribute selectors;
- sibling combinators;
- namespaces.

### Cascade

The cascade is resolved per property using:
- `!important`;
- inline-vs-author origin within the current supported model;
- selector specificity as `(id, class, tag)`;
- source order;
- declaration order.

Shorthands `margin` and `padding` expand to longhands before winning the cascade.

### Typed declarations

Unsupported declarations are discarded early.
Supported declarations become typed specified values before style computation.

### Memory

Computed styles are interned.

Instead of:

node -> full ComputedStyle
node -> full ComputedStyle
node -> full ComputedStyle

the snapshot trends toward:

node -> style index
node -> style index
node -> style index

styles -> unique ComputedStyle pool

Node lookup uses the numeric NodeId as a direct vector index.

## Supported properties

- display
- color
- background / background-color (color subset)
- font-size
- font-weight
- font-style
- font-family (coarse family)
- text-decoration / text-decoration-line
- margin + four longhands
- padding + four longhands
- width
- height

## Supported values

Lengths:
- 0
- px
- em
- rem
- percentages for width/height/font-size
- auto for width/height

Colors:
- basic named colors
- #rgb
- #rrggbb
- rgb(...)
- rgba(...)
- transparent

## Deliberately deferred

- external `<link rel="stylesheet">` resource loading;
- `@media`;
- CSS variables;
- `calc()`;
- min/max/clamp;
- borders;
- border radius;
- transforms;
- animations;
- Grid;
- full Flexbox properties;
- quirks mode.

External stylesheets should come only after the resource-loader boundary is explicit.
