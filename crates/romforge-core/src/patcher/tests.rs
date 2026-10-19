use super::plan::tag_of;

#[test]
fn tags_from_patch_names() {
    assert_eq!(tag_of("Mother3_v1.3_English.ips"), "Mother3 v1.3 English");
    assert_eq!(tag_of("patch.bps"), "patch");
}
