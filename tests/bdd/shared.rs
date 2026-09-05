//! Then steps both features use: the verdict, the exit code, a usage error.

use cucumber::then;

use super::world::DokWorld;

#[then(expr = "the verdict is {word}")]
fn verdict_is(w: &mut DokWorld, verdict: String) {
    // A conform report carries records; an assess report carries the verdict at the top.
    let actual = if w.report().get("records").is_some() {
        &w.record()["verdict"]
    } else {
        &w.report()["verdict"]
    };
    assert_eq!(actual.as_str(), Some(verdict.as_str()), "{}", w.report());
}

#[then(expr = "the exit code is {int}")]
fn exit_code_is(w: &mut DokWorld, code: i32) {
    assert_eq!(w.exit_code, Some(code), "stderr: {}", w.stderr);
}

#[then(expr = "dok exits with code {int} and reports {string}")]
fn exits_with_error(w: &mut DokWorld, code: i32, text: String) {
    assert_eq!(w.exit_code, Some(code), "stderr: {}", w.stderr);
    assert!(
        w.stderr.contains(&text),
        "stderr does not mention {text:?}: {}",
        w.stderr
    );
}
