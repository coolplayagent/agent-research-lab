//! Public report data and deterministic, side-effect-free Markdown rendering.
use contracts::{FindingKind, SourceAccessStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvidenceBindings {
    pub experiment_id: String,
    pub source_repository: String,
    pub source_commit: String,
    pub prompt_sha256: String,
    pub policy_sha256: String,
    #[serde(rename = "superpod_commit", alias = "knowledge_revision")]
    pub knowledge_revision: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceReference {
    pub id: String,
    pub title: String,
    /// Public HTTPS URL, or a source archive path relative to external knowledge repository's root.
    pub resource: String,
    pub sha256: Option<String>,
    pub retrieved_at: String,
    pub access_status: SourceAccessStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub kind: FindingKind,
    pub statement: String,
    pub source_ids: Vec<String>,
    pub method: String,
    pub limitations: String,
    /// A digest of an explicitly reviewed public evidence artifact, never a private run path.
    pub public_evidence_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutcomeReport {
    pub title: String,
    pub summary: String,
    pub bindings: EvidenceBindings,
    pub sources: Vec<SourceReference>,
    pub findings: Vec<Finding>,
    pub dissenting_views: Vec<String>,
    /// Required attestation by the publishing caller after reviewing the rendered text.
    pub publication_reviewed: bool,
}

fn yaml_quote(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization cannot fail")
}

pub fn render_report(report: &OutcomeReport) -> String {
    let mut text = format!(
        "---\ntype: Research Experiment\ntitle: {}\nstatus: emerging\ntags: [ai-sdlc, long-horizon-agents, agent-research]\nexperiment_id: {}\nsource_repository: {}\nsource_commit: {}\nprompt_sha256: {}\npolicy_sha256: {}\nsuperpod_baseline: {}\nsources:\n",
        yaml_quote(&report.title),
        yaml_quote(&report.bindings.experiment_id),
        yaml_quote(&report.bindings.source_repository),
        yaml_quote(&report.bindings.source_commit),
        yaml_quote(&report.bindings.prompt_sha256),
        yaml_quote(&report.bindings.policy_sha256),
        yaml_quote(&report.bindings.knowledge_revision)
    );
    if report.sources.is_empty() {
        text.push_str("  []\n");
    }
    for source in &report.sources {
        let resource = if source.resource.starts_with("https://") {
            source.resource.clone()
        } else {
            format!("../../../{}", source.resource)
        };
        text.push_str(&format!("  - id: {}\n    resource: {}\n    title: {}\n    retrieved_at: {}\n    access_status: {}\n", yaml_quote(&source.id), yaml_quote(&resource), yaml_quote(&source.title), yaml_quote(&source.retrieved_at), yaml_quote(source.access_status.as_str())));
        if let Some(digest) = &source.sha256 {
            text.push_str(&format!("    sha256: {}\n", yaml_quote(digest)));
        }
    }
    text.push_str(&format!(
        "---\n\n# {}\n\n{}\n\n## Findings\n",
        report.title, report.summary
    ));
    for finding in &report.findings {
        let kind = match finding.kind {
            FindingKind::Measured => "Measured",
            FindingKind::ExternalClaim => "External claim",
            FindingKind::Inference => "Inference",
            FindingKind::Unknown => "Unknown",
        };
        let citations = finding
            .source_ids
            .iter()
            .map(|id| format!("[^{id}]"))
            .collect::<Vec<_>>()
            .join(" ");
        text.push_str(&format!(
            "\n### {kind}\n\n{} {}\n\nMethod: {}\n\nLimitations: {}\n",
            finding.statement, citations, finding.method, finding.limitations
        ));
        if let Some(digest) = &finding.public_evidence_sha256 {
            text.push_str(&format!("\nPublic evidence SHA-256: `{digest}`\n"));
        }
    }
    if !report.dissenting_views.is_empty() {
        text.push_str("\n## Dissent and unresolved questions\n");
        for view in &report.dissenting_views {
            text.push_str(&format!("\n- {view}\n"));
        }
    }
    text.push_str(&format!("\n## Reproducibility\n\nExperiment `{}`; [source commit]({}/commit/{}); prompt SHA-256 `{}`; policy SHA-256 `{}`; external knowledge repository baseline `{}`.\n", report.bindings.experiment_id, report.bindings.source_repository.trim_end_matches('/'), report.bindings.source_commit, report.bindings.prompt_sha256, report.bindings.policy_sha256, report.bindings.knowledge_revision));
    for source in &report.sources {
        let resource = if source.resource.starts_with("https://") {
            source.resource.clone()
        } else {
            format!("../../../{}", source.resource)
        };
        let checksum = source
            .sha256
            .as_ref()
            .map(|digest| format!("SHA-256 `{digest}`."))
            .unwrap_or_else(|| "Content checksum unavailable.".into());
        text.push_str(&format!(
            "\n[^{}]: [{}](<{}>). Retrieved {}; {}. {}\n",
            source.id,
            source.title.replace(']', "\\]"),
            resource,
            source.retrieved_at,
            source.access_status,
            checksum
        ));
    }
    text
}

/// Stable report payload identity for callers that keep a publication outbox.
pub fn report_digest(report: &OutcomeReport) -> String {
    format!("{:x}", Sha256::digest(render_report(report).as_bytes()))
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
