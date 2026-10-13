//! Bounded CSS admission tests for Phantom 2O-10.

use phantom_css::Stylesheet;

#[test]
fn simple_rules_can_extend_beyond_the_previous_fixed_ceiling() {
    let css = (0..1_500)
        .map(|i| format!(".c{i}{{color:red}}"))
        .collect::<String>();
    let diagnostics = Stylesheet::parse(&css).diagnostics();
    assert_eq!(diagnostics.accepted_selector_rules, 1_500);
    assert_eq!(diagnostics.accepted_declarations, 1_500);
    assert!(!diagnostics.accepted_rule_limit_reached);
    assert!(!diagnostics.declaration_budget_exhausted);
}

#[test]
fn expanded_declarations_have_a_global_admission_budget() {
    let css = (0..2_000)
        .map(|i| format!(".c{i}{{margin:0; padding:0; border-width:1px;}}"))
        .collect::<String>();
    let diagnostics = Stylesheet::parse(&css).diagnostics();
    assert!(diagnostics.declaration_budget_exhausted);
    assert!(diagnostics.accepted_declarations <= 16_384);
    assert_eq!(diagnostics.accepted_declarations % 12, 0);
    assert!(diagnostics.accepted_selector_rules > 1_024);
    assert!(!diagnostics.accepted_rule_limit_reached);
}

#[test]
fn selector_ceiling_is_still_bounded() {
    let css = (0..3_000)
        .map(|i| format!(".c{i}{{color:red}}"))
        .collect::<String>();
    let diagnostics = Stylesheet::parse(&css).diagnostics();
    assert_eq!(diagnostics.accepted_selector_rules, 2_048);
    assert!(diagnostics.accepted_rule_limit_reached);
}
