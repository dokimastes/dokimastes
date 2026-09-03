//! Rendering a run: one row per probe, before anything is summarised.

use serde::Serialize;

use super::verdict::{Outcome, Verdict};

#[derive(Debug, Serialize)]
pub struct Record {
    pub id: String,
    pub claim: String,
    #[serde(flatten)]
    pub outcome: Outcome,
    pub verdict: Verdict,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub pack: String,
    pub repository: String,
    pub identity: String,
    pub expect: String,
    pub records: Vec<Record>,
}

impl Report {
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut c = (0, 0, 0);
        for r in &self.records {
            match r.verdict {
                Verdict::Pass => c.0 += 1,
                Verdict::Fail => c.1 += 1,
                Verdict::Unproven => c.2 += 1,
            }
        }
        c
    }

    pub fn all_pass(&self) -> bool {
        let (_, fail, unproven) = self.counts();
        fail == 0 && unproven == 0
    }

    pub fn markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "## Negative-capability run — `{}` on `{}` as `{}`, expecting {}\n\n",
            self.pack, self.repository, self.identity, self.expect
        ));
        out.push_str("| Probe | Claim | Verdict | What happened |\n|---|---|---|---|\n");
        for r in &self.records {
            out.push_str(&format!(
                "| {} | {} | **{}** | {} |\n",
                r.id,
                cell(&r.claim),
                verdict_label(r.verdict),
                cell(&r.note)
            ));
        }
        let (pass, fail, unproven) = self.counts();
        out.push_str(&format!(
            "\n**{pass} pass · {fail} fail · {unproven} unproven.** "
        ));
        out.push_str(if self.all_pass() {
            "Every claim in this pack was tried and behaved as expected.\n"
        } else {
            "A claim with no attempt behind it is unproven, not satisfied; a failed claim is a sentence in a document.\n"
        });
        out
    }
}

fn verdict_label(v: Verdict) -> &'static str {
    match v {
        Verdict::Pass => "pass",
        Verdict::Fail => "FAIL",
        Verdict::Unproven => "UNPROVEN",
    }
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}
