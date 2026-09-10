use crate::db::models::{Combo, ComboStep, ResolvedComboTarget, RoutingContext};
use crate::db::providers as db_providers;
use crate::error::{AppError, Result};
use rusqlite::Connection;

pub fn resolve_combo(
    conn: &Connection,
    combo: &Combo,
    context: &RoutingContext,
    engine: &crate::routing::RoutingEngine,
    enc_key: &[u8],
) -> Result<Vec<ResolvedComboTarget>> {
    let targets = resolve_targets(conn, &combo.data.models, enc_key)?;
    let ordered = engine.resolve(&combo.data.strategy, &targets, context);
    if ordered.is_empty() {
        return Err(AppError::Routing(format!(
            "Combo '{}' has no available targets",
            combo.name
        )));
    }
    Ok(ordered)
}

fn resolve_targets(conn: &Connection, steps: &[ComboStep], enc_key: &[u8]) -> Result<Vec<ResolvedComboTarget>> {
    let mut targets = Vec::new();

    for (i, step) in steps.iter().enumerate() {
        let step_id = format!("step-{}", i);

        let provider_id = if let Some((pid, _model)) = extract_provider_from_model(&step.model_str) {
            pid.to_string()
        } else {
            step.provider.clone()
        };

        let connection_id = if let Some(cid) = &step.connection_id {
            Some(cid.clone())
        } else {
            let connections = db_providers::list_by_provider(conn, &provider_id, enc_key)?;
            connections.first().map(|c| c.id.clone())
        };

        targets.push(ResolvedComboTarget {
            step_id,
            model_str: step.model_str.clone(),
            provider: step.provider.clone(),
            provider_id,
            connection_id,
            weight: step.weight,
            label: step.label.clone(),
            traffic_type: None,
        });
    }

    Ok(targets)
}

fn extract_provider_from_model(model_str: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = model_str.splitn(2, '/').collect();
    if parts.len() == 2 {
        Some((parts[0], parts[1]))
    } else {
        None
    }
}
