//! CSV schema of hardware captures — a contract between the firmware
//! (writer), labeling, and the training pipeline.
//!
//! Differs from the simulator export (`t_ms,current_a,state`) by its
//! tracing columns; `value` uses the same units (amps after
//! [`crate::calibration::CurrentCalibration`]), so the window-slicing and
//! feature code is shared between both tracks.

/// Capture CSV header (column order is fixed).
pub const CAPTURE_HEADER: [&str; 6] = ["t_ms", "node", "run_id", "value", "state", "note"];

/// `state` column value before labeling (labels are assigned manually from
/// the run log — the bench ground truth).
pub const STATE_UNLABELED: &str = "";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeKind;

    #[test]
    fn header_is_stable() {
        // The contract is pinned by this Literal: any change must be a
        // deliberate edit, in sync with the writer firmware and the
        // training script.
        assert_eq!(
            CAPTURE_HEADER,
            ["t_ms", "node", "run_id", "value", "state", "note"]
        );
    }

    #[test]
    fn node_column_values_match_node_kinds() {
        // The node column is in {a,p,q} — exactly the NodeKind::as_str values.
        let values = [
            NodeKind::A.as_str(),
            NodeKind::P.as_str(),
            NodeKind::Q.as_str(),
        ];
        assert_eq!(values, ["a", "p", "q"]);
    }
}
