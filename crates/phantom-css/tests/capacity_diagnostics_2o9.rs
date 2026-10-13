//! Regression tests for bounded CSS parser capacity diagnostics.
use phantom_css::Stylesheet;

#[test]
fn rejects_invalid_selectors_without_harming_following_rules() {
    let css = "x { color: red; } x::unsupported { color: blue; } y { color: green; }";
    let d = Stylesheet::parse(css).diagnostics();
    assert_eq!(d.scanned_rule_blocks, 3);
    assert_eq!(d.rejected_selector_blocks, 1);
    assert_eq!(d.accepted_selector_rules, 2);
    assert!(!d.accepted_rule_limit_reached);
}

#[test]
fn reports_limit_without_claiming_number_of_unexamined_rules() {
    let css = "a { color: red; }".repeat(1300);
    let d = Stylesheet::parse(&css).diagnostics();
    assert_eq!(d.accepted_selector_rules, 1300);
    assert!(!d.accepted_rule_limit_reached);

    let saturated = "a { color: red; }".repeat(2300);
    let d = Stylesheet::parse(&saturated).diagnostics();
    assert_eq!(d.accepted_selector_rules, 2048);
    assert!(d.accepted_rule_limit_reached);
}
