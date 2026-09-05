//! Given: a container with a git server, and whatever the scenario puts in it.

use cucumber::gherkin::Step;
use cucumber::given;

use super::world::DokWorld;
use crate::container::Container;

#[given("a git server with a repository holding branches main and side")]
fn git_server(w: &mut DokWorld) {
    let c = Container::start();
    let out = c.exec(&["provision-repo"], None);
    assert_eq!(out.code, 0, "provision-repo: {}", out.stderr);
    w.seed = Some(serde_json::from_str(&out.stdout).expect("verify-repo prints JSON"));
    w.container = Some(c);
}

#[given(expr = "the server refuses every push with {string}")]
fn server_refuses(w: &mut DokWorld, message: String) {
    let out = w.container().exec(&["refuse-pushes", &message], None);
    assert_eq!(out.code, 0, "refuse-pushes: {}", out.stderr);
}

#[given(expr = "the working tree contains {string}")]
fn working_tree_contains(w: &mut DokWorld, files: String) {
    let script = files
        .split(',')
        .map(str::trim)
        .map(|f| format!("mkdir -p \"$(dirname '{f}')\" && : > '{f}'"))
        .collect::<Vec<_>>()
        .join(" && ");
    let out = w
        .container()
        .exec(&["sh", "-c", &format!("cd /srv/work && {script}")], None);
    assert_eq!(out.code, 0, "{}", out.stderr);
}

#[given("a profile with")]
fn profile_with(w: &mut DokWorld, step: &Step) {
    let text = step
        .docstring
        .as_deref()
        .expect("a docstring with the profile")
        .to_string();
    let out = w
        .container()
        .exec(&["sh", "-c", "cat > /srv/profile.yaml"], Some(&text));
    assert_eq!(out.code, 0, "{}", out.stderr);
    w.profile_yaml = Some(text);
}
