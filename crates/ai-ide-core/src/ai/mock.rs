use crate::ai::provider::AiProvider;
use crate::error::IdeResult;
use crate::graph::is_test_file;
use crate::models::{ChangeKind, FilePatch, ImplementationPlan, PatchAction, TaskContext};
use crate::patch::PatchEngine;

use async_trait::async_trait;

pub struct MockAiProvider;

const DEFAULT_ROUTE: &str = "src/routes/users.ts";
const DEFAULT_TEST: &str = "tests/users.test.ts";

struct AddedField {
    name: String,
    ts_type: &'static str,
}

fn ts_type_for(value: Option<&serde_json::Value>) -> &'static str {
    match value.and_then(|v| v.as_str()) {
        Some("string") => "string",
        Some("number") | Some("integer") => "number",
        Some("boolean") => "boolean",
        _ => "unknown",
    }
}

fn placeholder_for(ts_type: &str) -> &'static str {
    match ts_type {
        "string" => "'active'",
        "number" => "0",
        "boolean" => "false",
        _ => "null",
    }
}

fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

/// Inserts `line` before the closing brace of the first block whose opening line matches `opener`.
fn insert_before_block_end(lines: &mut Vec<String>, opener: impl Fn(&str) -> bool, make: impl Fn(&str) -> String) -> bool {
    let Some(start) = lines.iter().position(|l| opener(l)) else {
        return false;
    };
    let base_indent = indent_of(&lines[start]).to_string();
    let Some(offset) = lines[start + 1..].iter().position(|l| {
        let t = l.trim_start();
        t.starts_with('}') && indent_of(l) == base_indent
    }) else {
        return false;
    };
    let end = start + 1 + offset;
    let inner_indent = format!("{}  ", base_indent);
    lines.insert(end, make(&inner_indent));
    true
}

fn patch_route(original: Option<&str>, fields: &[AddedField], node_id: &str) -> String {
    let Some(orig) = original else {
        let props: Vec<String> = fields
            .iter()
            .map(|f| format!("{}: {}", f.name, placeholder_for(f.ts_type)))
            .collect();
        return format!(
            "// Generated from {}\nexport const getUser = () => ({{ id: '1', {} }});\n",
            node_id,
            props.join(", ")
        );
    };

    let trailing_newline = orig.ends_with('\n');
    let mut lines: Vec<String> = orig.lines().map(str::to_string).collect();
    let mut changed = false;

    for f in fields.iter().filter(|f| !orig.contains(&format!("{}:", f.name))) {
        let typed = insert_before_block_end(
            &mut lines,
            |l| l.contains("interface ") && l.trim_end().ends_with('{'),
            |ind| format!("{}{}: {};", ind, f.name, f.ts_type),
        );
        let returned = insert_before_block_end(
            &mut lines,
            |l| l.trim_start().starts_with("return {") && l.trim_end().ends_with('{'),
            |ind| format!("{}{}: {},", ind, f.name, placeholder_for(f.ts_type)),
        );
        changed |= typed || returned;
    }

    if !changed {
        // Unrecognized shape: emit a minimal stub that satisfies the contract.
        return patch_route(None, fields, node_id);
    }
    let mut out = lines.join("\n");
    if trailing_newline {
        out.push('\n');
    }
    out
}

fn relative_from(test_path: &str, target: &str) -> String {
    let depth = test_path.matches('/').count();
    format!("{}{}", "../".repeat(depth), target)
}

fn patch_test(test_path: &str, original: Option<&str>, route: &str, fields: &[AddedField], spec_name: &str) -> String {
    let node_style = test_path.ends_with(".mjs") || test_path.ends_with(".js");
    if !node_style {
        let f = fields.first().map(|f| f.name.as_str()).unwrap_or("field");
        return format!(
            "test('{0} with {1}', () => {{ const u = {0}(); expect(u.id).toBe('1'); expect(u.{1}).toBeDefined(); }});\n",
            spec_name, f
        );
    }

    let mut out = original.unwrap_or("").to_string();
    if !out.contains("from 'node:test'") {
        out = format!("import test from 'node:test';\n{}", out);
    }
    if !out.contains("from 'node:assert") {
        out = format!("import assert from 'node:assert/strict';\n{}", out);
    }
    if !out.contains("node:fs/promises") {
        out = format!("import {{ readFile }} from 'node:fs/promises';\n{}", out);
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for f in fields {
        out.push_str(&format!(
            "\ntest('{name} implementation exposes {field} (spec contract)', async () => {{\n  const src = await readFile(new URL('{rel}', import.meta.url), 'utf8');\n  assert.match(src, /\\b{field}\\b/);\n}});\n",
            name = spec_name,
            field = f.name,
            rel = relative_from(test_path, route),
        ));
    }
    out
}

fn make_patch(path: &str, original: Option<&String>, patched: &str) -> FilePatch {
    let action = if original.is_some() { PatchAction::Modify } else { PatchAction::Create };
    PatchEngine::create_diff(path, action, original.map(String::as_str), patched)
}

#[async_trait]
impl AiProvider for MockAiProvider {
    fn name(&self) -> &str {
        "MockDeterministicProvider"
    }

    async fn generate_plan(&self, context: &TaskContext) -> IdeResult<ImplementationPlan> {
        let api_changes: Vec<_> = context
            .spec_changes
            .iter()
            .filter(|c| c.node_id.starts_with("api:"))
            .collect();

        let mut patches = Vec::new();
        let mut affected = Vec::new();

        if let Some(first) = api_changes.first() {
            let fields: Vec<AddedField> = api_changes
                .iter()
                .filter(|c| c.kind == ChangeKind::Added)
                .filter_map(|c| {
                    c.path.last().map(|name| AddedField {
                        name: name.clone(),
                        ts_type: ts_type_for(c.new_value.as_ref()),
                    })
                })
                .collect();
            let fields = if fields.is_empty() {
                vec![AddedField { name: "field".into(), ts_type: "unknown" }]
            } else {
                fields
            };
            let spec_name = first.node_id.trim_start_matches("api:");

            let mut impacted: Vec<&String> = context.impacted_files.keys().collect();
            impacted.sort();
            let route = impacted
                .iter()
                .find(|p| !is_test_file(p))
                .map(|p| p.as_str())
                .unwrap_or(DEFAULT_ROUTE)
                .to_string();
            let test = impacted
                .iter()
                .find(|p| is_test_file(p))
                .map(|p| p.as_str())
                .unwrap_or(DEFAULT_TEST)
                .to_string();

            let route_orig = context.impacted_files.get(&route);
            let route_new = patch_route(route_orig.map(String::as_str), &fields, &first.node_id);
            patches.push(make_patch(&route, route_orig, &route_new));
            affected.push(route.clone());

            let test_orig = context.impacted_files.get(&test);
            let test_new = patch_test(&test, test_orig.map(String::as_str), &route, &fields, spec_name);
            patches.push(make_patch(&test, test_orig, &test_new));
            affected.push(test.clone());
        }

        Ok(ImplementationPlan {
            summary: format!(
                "Generated targeted implementation for {} spec changes across {} files",
                context.spec_changes.len(),
                patches.len()
            ),
            rationale: "Deterministic template match fulfilling specification additions".into(),
            suggested_tests: affected.iter().filter(|p| is_test_file(p)).cloned().collect(),
            patches,
            affected_dependencies: affected,
            potential_risks: vec!["Ensure downstream API clients consume the new response shape".into()],
        })
    }

    async fn repair_patch(&self, context: &TaskContext, errors: &[String]) -> IdeResult<ImplementationPlan> {
        let mut plan = self.generate_plan(context).await?;
        plan.summary = format!("Repaired implementation targeting {} errors", errors.len());
        plan.rationale = format!("Targeted fix for failure: {}", errors.first().unwrap_or(&"unknown".to_string()));
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_route_inserts_into_interface_and_return() {
        let orig = "export interface UserResponse {\n  id: string;\n}\n\nexport function getUser(id: string): UserResponse {\n  return {\n    id,\n  };\n}\n";
        let out = patch_route(Some(orig), &[AddedField { name: "phone".into(), ts_type: "string" }], "api:getUser");
        assert!(out.contains("  id: string;\n  phone: string;\n}"));
        assert!(out.contains("    id,\n    phone: 'active',\n  };"));
    }

    #[test]
    fn test_patch_test_appends_node_test() {
        let orig = "import test from 'node:test';\nimport assert from 'node:assert/strict';\n";
        let out = patch_test(
            "tests/users.test.mjs",
            Some(orig),
            "src/routes/users.ts",
            &[AddedField { name: "phone".into(), ts_type: "string" }],
            "getUser",
        );
        assert!(out.starts_with("import { readFile } from 'node:fs/promises';\nimport test"));
        assert!(out.contains("new URL('../src/routes/users.ts', import.meta.url)"));
        assert!(out.contains("assert.match(src, /\\bphone\\b/);"));
    }
}
