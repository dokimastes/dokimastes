//! A container running the acceptance image, driven through the docker
//! or podman command line. One per scenario; removed on drop.

#![allow(dead_code)]

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

pub const IMAGE: &str = "dokimastes/bdd:local";

pub struct Exec {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub struct Container {
    runtime: &'static str,
    id: String,
}

fn runtime() -> &'static str {
    static RUNTIME: OnceLock<&'static str> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        if let Ok(explicit) = std::env::var("DOK_CONTAINER_RUNTIME") {
            return Box::leak(explicit.into_boxed_str());
        }
        for candidate in ["docker", "podman"] {
            let ok = Command::new(candidate).args(["version"]).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
            if ok {
                return candidate;
            }
        }
        panic!("neither docker nor podman is usable; the acceptance scenarios need one (set DOK_CONTAINER_RUNTIME to choose)");
    })
}

fn run(args: &[&str], stdin: Option<&str>) -> Exec {
    let mut cmd = Command::new(runtime());
    cmd.args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("{} {}: {e}", runtime(), args.join(" ")));
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    Exec {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

impl Container {
    /// Build the image from the working tree. Once per process.
    pub fn build_image() {
        static BUILT: OnceLock<()> = OnceLock::new();
        BUILT.get_or_init(|| {
            let root = env!("CARGO_MANIFEST_DIR");
            eprintln!("building {IMAGE} with {} from {root} …", runtime());
            let out = run(
                &[
                    "build",
                    "--quiet",
                    "-t",
                    IMAGE,
                    "-f",
                    &format!("{root}/tests/docker/Dockerfile"),
                    root,
                ],
                None,
            );
            assert_eq!(
                out.code, 0,
                "image build failed:\n{}\n{}",
                out.stdout, out.stderr
            );
        });
    }

    pub fn start() -> Container {
        let out = run(&["run", "-d", "--rm", IMAGE, "sleep", "infinity"], None);
        assert_eq!(out.code, 0, "container start failed: {}", out.stderr);
        Container {
            runtime: runtime(),
            id: out.stdout.trim().to_string(),
        }
    }

    pub fn exec(&self, args: &[&str], stdin: Option<&str>) -> Exec {
        let mut full = vec!["exec", "-i", self.id.as_str()];
        full.extend_from_slice(args);
        run(&full, stdin)
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        let _ = Command::new(self.runtime)
            .args(["rm", "-f", &self.id])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
