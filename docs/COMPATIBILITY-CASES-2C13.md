# 2C-13 Form Compatibility Cases

The milestone extends the WPT-ready testing discipline without importing the
entire WPT repository into ordinary CI.

Project-owned cases cover:

- current user value overriding the HTML value;
- hidden successful controls;
- disabled control exclusion;
- explicit submitter semantics;
- Enter submission without submitter;
- POST rejection rather than accidental GET downgrade;
- unsupported input-type exclusion;
- URL form encoding through rust-url.

These cases define the exact compatibility claim for Form Navigation I.
They are not a claim of HTML forms conformance.
