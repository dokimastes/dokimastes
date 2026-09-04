//! The example profile the docs point at must load, assess, and not be
//! refused — otherwise the documentation lies about the schema.

use dok::assess::measure::Measured;
use dok::assess::profile::{Profile, Verdict};
use dok::assess::rules::{assess, ModeCeiling};

#[test]
fn example_profile_is_amber_m3_staged_and_not_refused() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/examples/profile.yaml"
    ))
    .unwrap();
    let profile = Profile::from_yaml(&text).unwrap();
    let a = assess(&profile, &Measured::default());
    assert_eq!(a.verdict, Verdict::Amber, "{:#?}", a.findings);
    assert_eq!(a.ceiling, ModeCeiling::M3Staged);
    assert!(a.refusals.is_empty(), "{:?}", a.refusals);
}
