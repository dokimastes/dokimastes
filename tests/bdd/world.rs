//! The state a scenario carries between steps.

use cucumber::World;
use serde_json::Value;

use crate::container::Container;

#[derive(Debug, Default, World)]
pub struct DokWorld {
    pub container: Option<Container>,
    /// Repository state right after provisioning, from `verify-repo`.
    pub seed: Option<Value>,
    pub profile_yaml: Option<String>,
    /// The JSON report of the last `dok` invocation, when it produced one.
    pub report: Option<Value>,
    pub exit_code: Option<i32>,
    pub stderr: String,
}

impl DokWorld {
    pub fn container(&self) -> &Container {
        self.container.as_ref().expect("a git server first")
    }

    pub fn report(&self) -> &Value {
        self.report
            .as_ref()
            .unwrap_or_else(|| panic!("no report; stderr was: {}", self.stderr))
    }

    /// The one probe record of a `dok conform --only <id>` run.
    pub fn record(&self) -> &Value {
        &self.report()["records"][0]
    }

    /// Run `dok` in the container and keep its report, exit code and stderr.
    pub fn run_dok(&mut self, args: &[&str]) {
        let mut full = vec!["dok"];
        full.extend_from_slice(args);
        let out = self.container().exec(&full, None);
        self.exit_code = Some(out.code);
        self.stderr = out.stderr;
        self.report = serde_json::from_str(&out.stdout).ok();
    }
}
