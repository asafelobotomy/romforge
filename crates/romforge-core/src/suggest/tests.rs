#[test]
fn codes_are_whole_parts_of_serials() {
    use super::facts::code_in;
    assert!(code_in("AQ3J", "SHVC-AQ3J-JPN"));
    assert!(code_in("GALE", "DL-DOL-GALE-USA"));
    assert!(!code_in("CAQ3", "SHVC-AQ3J-JPN"), "across two parts");
    assert!(!code_in("AQ3J", "SHVCAQ3JJPN"), "no parts to tell");
}
