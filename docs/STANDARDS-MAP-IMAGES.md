# Phantom Standards Map — Images and Replaced Elements

This file records the standards basis used by the 2C image milestones.

## HTML image element

WHATWG HTML:

https://html.spec.whatwg.org/multipage/images.html

Source:

https://github.com/whatwg/html

Important implementation topics:

- `img`
- source selection
- natural dimensions
- density correction
- width/height dimension attributes
- alt text
- image availability state

## HTML rendering model

WHATWG HTML Rendering:

https://html.spec.whatwg.org/multipage/rendering.html

Important topic:

- replaced elements

The HTML rendering model identifies `img` among elements that can be treated as
replaced elements.

## CSS Images

CSS Images Module:

https://drafts.csswg.org/css-images/

Source family:

https://github.com/w3c/csswg-drafts

Important topics:

- natural dimensions
- concrete object size
- default object size
- object size negotiation

## CSS sizing and replaced elements

CSS Sizing:

https://drafts.csswg.org/css-sizing-3/

CSS 2 replaced sizing foundation:

https://drafts.csswg.org/css2/

Important topics:

- auto width/height
- intrinsic dimensions
- intrinsic ratio
- min/max constraints
- replaced elements

## Permanent rule

When Phantom behavior is uncertain:

1. inspect current normative HTML/CSS text;
2. inspect relevant WPT;
3. inspect CSSWG/WHATWG issues if text is ambiguous or actively changing;
4. compare interoperable browser behavior;
5. record any intentional deviation.

Do not copy one browser's behavior merely because it is convenient.

## 2C-1 supported subset

The 2C-1 implementation is intentionally smaller than the standards:

- `<img>` replaced box
- width/height attributes
- intrinsic metadata catalog
- natural aspect ratio
- default object-size foundation
- CSS width/height subset
- min/max constraints
- image PaintCommand boundary

Everything else remains explicitly unsupported until implemented and tested.
