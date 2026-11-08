use minijinja::syntax::SyntaxConfig;
use minijinja::value::Serde;
use minijinja::{
    Environment,
    UndefinedBehavior,
};

use crate::spec::Spec;

const TEMPLATES: [(&str, &str); 11] = [
    (
        "Cargo.toml.j2",
        include_str!("../../templates/plugin/Cargo.toml.j2"),
    ),
    (
        "icon.svg.j2",
        include_str!("../../templates/plugin/icon.svg.j2"),
    ),
    (
        "lib.rs.j2",
        include_str!("../../templates/plugin/lib.rs.j2"),
    ),
    (
        "metadata.rs.j2",
        include_str!("../../templates/plugin/metadata.rs.j2"),
    ),
    (
        "schema.rs.j2",
        include_str!("../../templates/plugin/schema.rs.j2"),
    ),
    (
        "config.rs.j2",
        include_str!("../../templates/plugin/config.rs.j2"),
    ),
    (
        "config_tests.rs.j2",
        include_str!("../../templates/plugin/config_tests.rs.j2"),
    ),
    (
        "client.rs.j2",
        include_str!("../../templates/plugin/client.rs.j2"),
    ),
    (
        "rest_plugin.rs.j2",
        include_str!("../../templates/plugin/rest_plugin.rs.j2"),
    ),
    (
        "kubernetes_plugin.rs.j2",
        include_str!("../../templates/plugin/kubernetes_plugin.rs.j2"),
    ),
    (
        "stubs.rs.j2",
        include_str!("../../templates/plugin/stubs.rs.j2"),
    ),
];

fn quote_list(values: Vec<String>) -> String {
    values
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

fn environment() -> Result<Environment<'static>, String> {
    let mut env = Environment::new();
    let syntax = SyntaxConfig::builder()
        .trim_blocks(true)
        .lstrip_blocks(true)
        .keep_trailing_newline(true)
        .build()
        .map_err(|e| e.to_string())?;
    env.set_syntax(syntax);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.add_filter("quote_list", quote_list);
    for (name, source) in TEMPLATES {
        env.add_template(name, source)
            .map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(env)
}

pub(crate) fn files(spec: &Spec) -> Result<Vec<(String, String)>, String> {
    let env = environment()?;
    let render = |template: &str| {
        env.get_template(template)
            .and_then(|template| template.render(Serde(spec)))
            .map_err(|e| format!("{template}: {e:#}"))
    };
    let mut outputs = vec![
        ("Cargo.toml", "Cargo.toml.j2"),
        ("src/lib.rs", "lib.rs.j2"),
        ("src/metadata.rs", "metadata.rs.j2"),
        ("src/schema.rs", "schema.rs.j2"),
        ("src/config.rs", "config.rs.j2"),
        ("src/config/tests.rs", "config_tests.rs.j2"),
    ];
    if spec.kubernetes {
        outputs.push(("src/plugin.rs", "kubernetes_plugin.rs.j2"));
    } else {
        outputs.push(("src/client.rs", "client.rs.j2"));
        outputs.push(("src/plugin.rs", "rest_plugin.rs.j2"));
    }
    let mut files = vec![(
        "assets/icon.svg".to_string(),
        match &spec.icon_svg {
            Some(svg) => svg.clone(),
            None => render("icon.svg.j2")?,
        },
    )];
    for (path, template) in outputs {
        files.push((path.to_string(), render(template)?));
    }
    Ok(files)
}

#[cfg(test)]
mod tests;
