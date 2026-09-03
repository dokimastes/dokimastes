//! The pack this repository ships must load, and every probe must name the
//! identities and mechanisms the discipline requires.

use dok::conform::spec::{Identity, Pack};

#[test]
fn shipped_pack_loads_and_covers_the_rehearsal_probe_set() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/conformance/negative-capability/pack.yaml"
    ))
    .unwrap();
    let pack = Pack::from_yaml(&text).unwrap();
    assert_eq!(pack.target.repository, "dokimastes/dokimastes");
    let ids: Vec<&str> = pack.probes.iter().map(|p| p.id.as_str()).collect();
    for required in [
        "NC-01", "NC-02", "NC-03", "NC-04", "NC-05", "NC-06", "NC-07", "NC-08", "NC-09", "NC-10",
    ] {
        assert!(ids.contains(&required), "missing {required}");
    }
    let admin_probe = pack.probes.iter().find(|p| p.id == "NC-09").unwrap();
    assert_eq!(
        admin_probe.run_as,
        vec![Identity::RepoAdmin],
        "NC-09 is meaningless from any other account"
    );
    for p in &pack.probes {
        assert!(
            !p.stories.is_empty(),
            "{}: every claim traces to a backlog story",
            p.id
        );
    }
}
