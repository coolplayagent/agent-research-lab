//! Resolve a configured route without printing the composed config or its secrets.
use anyhow::{Context, Result, ensure};
use serde_yaml::Value;
use service_api::ResolvedAgent;
use std::collections::BTreeSet;
pub fn selected_model(agent: &ResolvedAgent) -> Result<String> {
    let mut args = agent.arguments.clone();
    args.push("--dump-config".into());
    let mut child = crate::process::Session::spawn(
        &agent.executable,
        &args,
        &std::env::temp_dir(),
        &[],
        5,
        false,
    )?;
    child.close_input();
    let mut document = String::new();
    while let Some(line) = child.next()? {
        ensure!(
            document.len() + line.len() <= 1024 * 1024,
            "agent configuration dump exceeds bound"
        );
        document.push_str(&line);
    }
    child.wait()?;
    route(&document)
}
fn route(document: &str) -> Result<String> {
    let value: Value = serde_yaml::from_str(document)
        .context("DeepSeek Harness configuration could not be read")?;
    let mut routes = BTreeSet::new();
    collect(&value, &mut routes, 0)?;
    ensure!(
        routes.len() == 1,
        "DeepSeek Harness has no unambiguous configured provider/model; provide an explicit override"
    );
    routes
        .into_iter()
        .next()
        .context("configured model is missing")
}
fn collect(value: &Value, routes: &mut BTreeSet<String>, depth: usize) -> Result<()> {
    ensure!(depth <= 64, "agent configuration exceeds nesting bound");
    match value {
        Value::Mapping(map) => {
            if let Some(Value::Sequence(agents)) = map.get(Value::String("agents".into())) {
                for agent in agents {
                    if let (Value::String(provider), Value::String(model)) =
                        (&agent["provider"], &agent["model"])
                    {
                        ensure!(
                            !provider.is_empty()
                                && !model.is_empty()
                                && provider.len() + model.len() <= 199,
                            "invalid configured model route"
                        );
                        routes.insert(format!("{provider}/{model}"));
                    }
                }
            }
            for value in map.values() {
                collect(value, routes, depth + 1)?;
            }
        }
        Value::Sequence(values) => {
            for value in values {
                collect(value, routes, depth + 1)?;
            }
        }
        Value::Tagged(_) => {} // Executable YAML expressions are never evaluated here.
        _ => {}
    }
    Ok(())
}
#[cfg(test)]
#[path = "dsh_config_tests.rs"]
mod tests;
