//! Port of src/scoring.ts. Pure function, no I/O.
//! spec: risk-scoring#S1-S4 — a skipped check's findings never count, even
//! if one somehow carries points (defensive, not just trusted of callers).

use crate::types::{CheckStatus, Finding, RiskLevel, ScoreResult};

pub struct ScorableResult<'a> {
    pub status: CheckStatus,
    pub findings: &'a [Finding],
}

fn level_for(total: u32) -> RiskLevel {
    if total >= 50 {
        RiskLevel::High
    } else if total >= 20 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

pub fn calculate_score(results: &[ScorableResult]) -> ScoreResult {
    let findings: Vec<Finding> = results
        .iter()
        .filter(|r| r.status != CheckStatus::Skipped)
        .flat_map(|r| r.findings.iter().cloned())
        .collect();

    let total: u32 = findings.iter().map(|f| f.points).sum();

    ScoreResult {
        total,
        level: level_for(total),
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(points: u32, id: &str) -> Finding {
        Finding {
            id: id.to_string(),
            severity: crate::types::Severity::Warn,
            points,
            title: id.to_string(),
            detail: None,
        }
    }

    // spec: risk-scoring#S1
    #[test]
    fn zero_point_findings_score_low() {
        let findings = vec![finding(0, "a")];
        let score = calculate_score(&[ScorableResult {
            status: CheckStatus::Ok,
            findings: &findings,
        }]);
        assert_eq!(score.total, 0);
        assert_eq!(score.level, RiskLevel::Low);
    }

    // spec: risk-scoring#S2
    #[test]
    fn skipped_check_contributes_nothing() {
        let skipped_findings = vec![finding(40, "should-not-count")];
        let ok_findings = vec![finding(0, "b")];
        let score = calculate_score(&[
            ScorableResult {
                status: CheckStatus::Skipped,
                findings: &skipped_findings,
            },
            ScorableResult {
                status: CheckStatus::Ok,
                findings: &ok_findings,
            },
        ]);
        assert_eq!(score.total, 0);
        assert_eq!(score.level, RiskLevel::Low);
        assert!(!score.findings.iter().any(|f| f.id == "should-not-count"));
    }

    // spec: risk-scoring#S3
    #[test]
    fn nineteen_is_low_twenty_is_medium() {
        let f19 = vec![finding(19, "a")];
        let f20 = vec![finding(20, "a")];
        let low = calculate_score(&[ScorableResult {
            status: CheckStatus::Ok,
            findings: &f19,
        }]);
        let medium = calculate_score(&[ScorableResult {
            status: CheckStatus::Ok,
            findings: &f20,
        }]);
        assert_eq!(low.level, RiskLevel::Low);
        assert_eq!(medium.level, RiskLevel::Medium);
    }

    // spec: risk-scoring#S4
    #[test]
    fn forty_nine_is_medium_fifty_is_high() {
        let f49 = vec![finding(49, "a")];
        let f50 = vec![finding(50, "a")];
        let medium = calculate_score(&[ScorableResult {
            status: CheckStatus::Ok,
            findings: &f49,
        }]);
        let high = calculate_score(&[ScorableResult {
            status: CheckStatus::Ok,
            findings: &f50,
        }]);
        assert_eq!(medium.level, RiskLevel::Medium);
        assert_eq!(high.level, RiskLevel::High);
    }
}
