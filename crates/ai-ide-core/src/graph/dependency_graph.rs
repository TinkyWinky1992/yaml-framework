use crate::error::{IdeError, IdeResult};
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeType {
    Implements,  // Source code implements Spec
    References,  // Spec A references Spec B or Source A references Source B
    Tests,       // Test file verifies Source code or Spec
    Imports,     // Source file imports another file
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String, // e.g. "spec:api:getUser", "file:src/routes/users.ts", "test:tests/users.test.ts"
    pub file_path: String,
    pub is_spec: bool,
    pub is_test: bool,
}

#[derive(Debug, Clone)]
pub struct DependencyGraph {
    graph: DiGraph<GraphNode, EdgeType>,
    node_indices: HashMap<String, NodeIndex>,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
        }
    }

    pub fn add_or_get_node(&mut self, id: &str, file_path: &str, is_spec: bool, is_test: bool) -> NodeIndex {
        if let Some(&idx) = self.node_indices.get(id) {
            return idx;
        }

        let node = GraphNode {
            id: id.to_string(),
            file_path: file_path.to_string(),
            is_spec,
            is_test,
        };
        let idx = self.graph.add_node(node);
        self.node_indices.insert(id.to_string(), idx);
        idx
    }

    pub fn add_edge(&mut self, from_id: &str, to_id: &str, edge_type: EdgeType) -> IdeResult<()> {
        let from_idx = *self.node_indices.get(from_id)
            .ok_or_else(|| IdeError::DependencyGraph(format!("Source node '{}' not found in graph", from_id)))?;
        let to_idx = *self.node_indices.get(to_id)
            .ok_or_else(|| IdeError::DependencyGraph(format!("Target node '{}' not found in graph", to_id)))?;

        let edge = self.graph.add_edge(from_idx, to_idx, edge_type);

        if is_cyclic_directed(&self.graph) {
            self.graph.remove_edge(edge);
            return Err(IdeError::CircularDependency(format!(
                "Adding edge from '{}' to '{}' created a cycle in the dependency graph",
                from_id, to_id
            )));
        }

        Ok(())
    }

    pub fn get_node(&self, id: &str) -> Option<&GraphNode> {
        self.node_indices.get(id).map(|&idx| &self.graph[idx])
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Computes transitive impact sets when a set of spec nodes change.
    /// Traverses outgoing edges to find implementations, transitive dependents, and tests.
    pub fn compute_impact(
        &self,
        changed_spec_ids: &[String],
        all_project_files: &[String],
    ) -> (Vec<String>, Vec<String>, Vec<String>, usize) {
        let mut directly_affected = HashSet::new();
        let mut transitively_affected = HashSet::new();
        let mut verification_only = HashSet::new();

        for spec_id in changed_spec_ids {
            if let Some(&start_idx) = self.node_indices.get(spec_id) {
                // Direct dependents (outgoing edges from spec: source files that implement it)
                let mut direct_neighbors = Vec::new();
                for neighbor in self.graph.neighbors_directed(start_idx, Direction::Outgoing) {
                    let node = &self.graph[neighbor];
                    if node.is_test {
                        verification_only.insert(node.file_path.clone());
                    } else if !node.is_spec {
                        directly_affected.insert(node.file_path.clone());
                        direct_neighbors.push(neighbor);
                    }
                }

                // Transitive dependents (downstream consumers of the directly affected files)
                for direct_idx in direct_neighbors {
                    let mut queue = vec![direct_idx];
                    let mut visited = HashSet::new();
                    visited.insert(direct_idx);

                    while let Some(current) = queue.pop() {
                        for downstream in self.graph.neighbors_directed(current, Direction::Outgoing) {
                            if visited.insert(downstream) {
                                let node = &self.graph[downstream];
                                if node.is_test {
                                    verification_only.insert(node.file_path.clone());
                                } else if !node.is_spec && !directly_affected.contains(&node.file_path) {
                                    transitively_affected.insert(node.file_path.clone());
                                    queue.push(downstream);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut direct_vec: Vec<String> = directly_affected.into_iter().collect();
        let mut trans_vec: Vec<String> = transitively_affected.into_iter().collect();
        let mut verify_vec: Vec<String> = verification_only.into_iter().collect();

        direct_vec.sort();
        trans_vec.sort();
        verify_vec.sort();

        let mut affected_all = HashSet::new();
        for f in &direct_vec { affected_all.insert(f.clone()); }
        for f in &trans_vec { affected_all.insert(f.clone()); }
        for f in &verify_vec { affected_all.insert(f.clone()); }

        let unaffected_count = all_project_files.iter()
            .filter(|f| !affected_all.contains(*f))
            .count();

        (direct_vec, trans_vec, verify_vec, unaffected_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_impact_analysis() {
        let mut graph = DependencyGraph::new();

        let spec = "spec:api:getUser";
        let route = "file:src/routes/users.ts";
        let client = "file:src/client/users.ts";
        let test = "test:tests/users.test.ts";

        graph.add_or_get_node(spec, "specs/api/users.yaml", true, false);
        graph.add_or_get_node(route, "src/routes/users.ts", false, false);
        graph.add_or_get_node(client, "src/client/users.ts", false, false);
        graph.add_or_get_node(test, "tests/users.test.ts", false, true);

        graph.add_edge(spec, route, EdgeType::Implements).unwrap();
        graph.add_edge(route, client, EdgeType::Imports).unwrap();
        graph.add_edge(route, test, EdgeType::Tests).unwrap();

        let all_files = vec![
            "src/routes/users.ts".into(),
            "src/client/users.ts".into(),
            "tests/users.test.ts".into(),
            "src/unrelated/logger.ts".into(),
        ];

        let (direct, trans, verify, unaffected) = graph.compute_impact(&[spec.to_string()], &all_files);

        assert_eq!(direct, vec!["src/routes/users.ts"]);
        assert_eq!(trans, vec!["src/client/users.ts"]);
        assert_eq!(verify, vec!["tests/users.test.ts"]);
        assert_eq!(unaffected, 1);
    }

    #[test]
    fn test_rejected_cycle_edge_is_not_kept() {
        let mut graph = DependencyGraph::new();
        graph.add_or_get_node("a", "a.ts", false, false);
        graph.add_or_get_node("b", "b.ts", false, false);
        graph.add_edge("a", "b", EdgeType::Imports).unwrap();

        assert!(matches!(
            graph.add_edge("b", "a", EdgeType::Imports),
            Err(IdeError::CircularDependency(_))
        ));
        assert_eq!(graph.edge_count(), 1);
    }
}
