//! Acceptance scenarios, in Gherkin, executed for real: the `dok` binary
//! runs inside a container built from this working tree, against a git
//! repository provisioned in that container. Given provisions, When runs
//! the command, Then reads the command's JSON report — and, for the
//! repository's state, makes one `verify-repo` call.
//!
//! Needs docker or podman. Without one the scenarios fail; they do not
//! skip, because a claim with no attempt behind it is unproven.
//!
//! Steps are split by what they do: `provisioning` holds the shared Given
//! steps, `conform`, `assess` and `baseline` the steps of their feature, and
//! `shared` the Then steps every feature uses.

mod container;

mod bdd {
    pub mod assess;
    pub mod baseline;
    pub mod conform;
    pub mod provisioning;
    pub mod shared;
    pub mod world;
}

use cucumber::World;

use bdd::world::DokWorld;
use container::Container;

#[tokio::main]
async fn main() {
    Container::build_image();
    DokWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("conformance/features")
        .await;
}
