//! Regression coverage for bounded CSS parser diagnostics (2O-7 R1).

use phantom_css::Stylesheet;

#[test]
fn counts_are_based_on_accepted_rules_without_modifying_cascade() {
    let source = "p, div { color: red; margin: 1px; } bad::pseudo { color: blue; }";
    let sheet = Stylesheet::parse(source);
    let counters = sheet.diagnostics();
    assert_eq!(counters.accepted_selector_rules, sheet.len());
    assert_eq!(counters.accepted_selector_rules, 2);
    // `margin` expands into four sides: (color + 4 margins) * 2 selectors.
    assert_eq!(counters.accepted_declarations, 10);
    assert_eq!(counters.scanned_rule_blocks, 2);
}

#[test]
fn oversized_css_is_rejected_with_empty_counts() {
    let source = "x".repeat(1024 * 1024 + 1);
    let sheet = Stylesheet::parse(&source);
    assert!(sheet.is_empty());
    assert_eq!(sheet.diagnostics().scanned_rule_blocks, 0);
    assert_eq!(sheet.diagnostics().accepted_declarations, 0);
}
