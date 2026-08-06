#[test]
fn governed_scan_authority_results_are_not_caller_constructible() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/governed_scan_result_fields_are_private.rs");
}
