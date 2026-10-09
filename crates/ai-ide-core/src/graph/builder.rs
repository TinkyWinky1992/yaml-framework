use crate::error::IdeResult;
use crate::graph::{DependencyGraph, EdgeType};
use crate::models::SpecNode;
use crate::workspace::WorkspaceManager;

const SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs"];

pub fn is_test_file(path: &str) -> bool {
    path.starts_with("tests/") || path.contains(".test.") || path.contains(".spec.")
}

pub fn graph_node_id(path: &str) -> String {
    if is_test_file(path) {
        format!("test:{}", path)
    } else {
        format!("file:{}", path)
    }
}

fn contains_identifier(haystack: &str, ident: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    haystack.match_indices(ident).any(|(i, _)| {
        let before = haystack[..i].chars().next_back();
        let after = haystack[i + ident.len()..].chars().next();
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
}

pub fn declares_identifier(content: &str, ident: &str) -> bool {
    ["function ", "function* ", "const ", "let ", "var ", "class "]
        .iter()
        .any(|kw| contains_identifier(content, &format!("{}{}", kw, ident)))
}

fn strip_source_ext(path: &str) -> &str {
    for ext in SOURCE_EXTENSIONS {
        if let Some(stem) = path.strip_suffix(&format!(".{}", ext)) {
            return stem;
        }
    }
    path
}

/// Extracts relative module specifiers from `from '...'`, `import '...'`, `import('...')`, `require('...')`.
fn relative_imports(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in ["from ", "import ", "import(", "require("] {
        for (i, _) in content.match_indices(marker) {
            let rest = content[i + marker.len()..].trim_start();
            let Some(quote) = rest.chars().next().filter(|c| matches!(c, '\'' | '"' | '`')) else {
                continue;
            };
            if let Some(end) = rest[1..].find(quote) {
                let spec = &rest[1..1 + end];
                if spec.starts_with("./") || spec.starts_with("../") {
                    out.push(spec.to_string());
                }
            }
        }
    }
    out
}

/// Resolves a relative specifier against the importing file, returning an extension-less path.
fn resolve_import(importer: &str, specifier: &str) -> String {
    let mut parts: Vec<&str> = importer.split('/').collect();
    parts.pop();
    for seg in specifier.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    strip_source_ext(&parts.join("/")).to_string()
}

impl DependencyGraph {
    /// Builds the spec → source → test graph by scanning `src/` and `tests/`.
    /// Returns the graph and the list of project files considered.
    pub fn build_from_workspace(wm: &WorkspaceManager, specs: &[&SpecNode]) -> IdeResult<(Self, Vec<String>)> {
        let mut files = wm.list_files_under("src")?;
        files.extend(wm.list_files_under("tests")?);

        let sources: Vec<(String, String)> = files
            .iter()
            .filter(|f| {
                f.rsplit('.')
                    .next()
                    .is_some_and(|ext| SOURCE_EXTENSIONS.contains(&ext))
            })
            .filter_map(|f| wm.read_file(f).ok().map(|c| (f.clone(), c)))
            .collect();

        let mut graph = DependencyGraph::new();
        for spec in specs {
            graph.add_or_get_node(&spec.id, &spec.file_path, true, false);
        }
        for (path, _) in &sources {
            graph.add_or_get_node(&graph_node_id(path), path, false, is_test_file(path));
        }

        // Cycles are rejected by add_edge and simply skipped here.
        for spec in specs {
            let implementers: Vec<&String> = sources
                .iter()
                .filter(|(p, c)| !is_test_file(p) && declares_identifier(c, &spec.name))
                .map(|(p, _)| p)
                .collect();

            for path in &implementers {
                let _ = graph.add_edge(&spec.id, &graph_node_id(path), EdgeType::Implements);
            }

            for (test_path, content) in sources.iter().filter(|(p, _)| is_test_file(p)) {
                if !contains_identifier(content, &spec.name) {
                    continue;
                }
                let test_id = graph_node_id(test_path);
                if implementers.is_empty() {
                    let _ = graph.add_edge(&spec.id, &test_id, EdgeType::Tests);
                }
                for path in &implementers {
                    let _ = graph.add_edge(&graph_node_id(path), &test_id, EdgeType::Tests);
                }
            }
        }

        for (importer, content) in &sources {
            for spec in relative_imports(content) {
                let target = resolve_import(importer, &spec);
                if let Some((dep, _)) = sources.iter().find(|(p, _)| strip_source_ext(p) == target) {
                    if dep != importer {
                        let edge = if is_test_file(importer) { EdgeType::Tests } else { EdgeType::Imports };
                        let _ = graph.add_edge(&graph_node_id(dep), &graph_node_id(importer), edge);
                    }
                }
            }
        }

        Ok((graph, files))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::specs::SpecParser;

    #[test]
    fn test_build_graph_links_spec_source_import_and_test() {
        let dir = std::env::temp_dir().join(format!("ai_ide_graph_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let wm = WorkspaceManager::new(&dir).unwrap();

        wm.write_file("src/routes/users.ts", "export function getUser() { return { id: '1' }; }\n").unwrap();
        wm.write_file("src/client/users.ts", "import { getUser } from '../routes/users';\nexport const load = getUser;\n").unwrap();
        wm.write_file("src/unrelated.ts", "export const getUserAgent = 1;\n").unwrap();
        wm.write_file("tests/users.test.mjs", "test('getUser works', () => {});\n").unwrap();

        let spec = SpecParser::parse_yaml_str("api:\n  name: getUser\n", "specs/api/users.yaml").unwrap();
        let (graph, files) = DependencyGraph::build_from_workspace(&wm, &[&spec]).unwrap();
        let (direct, trans, verify, unaffected) = graph.compute_impact(&[spec.id.clone()], &files);

        assert_eq!(direct, vec!["src/routes/users.ts"]);
        assert_eq!(trans, vec!["src/client/users.ts"]);
        assert_eq!(verify, vec!["tests/users.test.mjs"]);
        assert_eq!(unaffected, 1);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_resolve_import() {
        assert_eq!(resolve_import("src/client/users.ts", "../routes/users.js"), "src/routes/users");
        assert_eq!(resolve_import("tests/a.test.mjs", "./helpers"), "tests/helpers");
    }
}
