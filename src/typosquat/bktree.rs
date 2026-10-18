//! BK-tree index over the popular-package dataset.
//!
//! [`nearest_match`](super::distance::nearest_match) compares a candidate
//! against every dataset entry, which is fine for a couple of thousand names
//! but not for full registry lists (100k+). A BK-tree (Burkhard–Keller tree)
//! exploits the triangle inequality of [`levenshtein_distance`]: each child
//! edge is labelled with its distance to the parent, so a search for names
//! within `max` of a candidate only has to descend into children whose edge
//! label lies in `d - max ..= d + max`, where `d` is the candidate's distance
//! to the current node. Small radii (the scanner uses 2) prune most of the
//! tree.

use super::distance::levenshtein_distance;

/// One dataset name stored in the tree.
struct Node<'a> {
    name: &'a str,
    /// Position of `name` in the dataset the tree was built from, used to
    /// break ties the same way a linear scan does.
    index: usize,
    /// `(edge distance, child node index)` pairs. Edge distances are unique
    /// among a node's children.
    children: Vec<(usize, usize)>,
}

/// A BK-tree over a borrowed list of names, keyed by
/// [`levenshtein_distance`].
pub struct BkTree<'a> {
    nodes: Vec<Node<'a>>,
}

impl<'a> BkTree<'a> {
    /// Builds a tree from `dataset`, inserting names in order.
    ///
    /// Duplicate names are stored once, at the position of their first
    /// occurrence, matching how a linear scan would report them.
    pub fn new(dataset: &'a [String]) -> Self {
        let mut tree = BkTree {
            nodes: Vec::with_capacity(dataset.len()),
        };
        for (index, name) in dataset.iter().enumerate() {
            tree.insert(name, index);
        }
        tree
    }

    fn insert(&mut self, name: &'a str, index: usize) {
        let new_node = Node {
            name,
            index,
            children: Vec::new(),
        };
        if self.nodes.is_empty() {
            self.nodes.push(new_node);
            return;
        }

        let mut current = 0;
        loop {
            let distance = levenshtein_distance(name, self.nodes[current].name);
            if distance == 0 {
                return; // duplicate: keep the earlier entry
            }
            match self.nodes[current].children.iter().find(|&&(edge, _)| edge == distance) {
                Some(&(_, child)) => current = child,
                None => {
                    let child = self.nodes.len();
                    self.nodes.push(new_node);
                    self.nodes[current].children.push((distance, child));
                    return;
                }
            }
        }
    }

    /// Number of distinct names in the tree.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True if the tree holds no names.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns every name within `max` edits of `candidate`, with its
    /// distance, sorted by distance and then by dataset order — the same
    /// result a brute-force scan filtered to `distance <= max` would give.
    pub fn within(&self, candidate: &str, max: usize) -> Vec<(&'a str, usize)> {
        if self.nodes.is_empty() {
            return Vec::new();
        }

        let mut found: Vec<(usize, &'a str, usize)> = Vec::new();
        let mut stack = vec![0];
        while let Some(current) = stack.pop() {
            let node = &self.nodes[current];
            let distance = levenshtein_distance(candidate, node.name);
            if distance <= max {
                found.push((node.index, node.name, distance));
            }
            let low = distance.saturating_sub(max);
            let high = distance.saturating_add(max);
            stack.extend(
                node.children
                    .iter()
                    .filter(|&&(edge, _)| (low..=high).contains(&edge))
                    .map(|&(_, child)| child),
            );
        }

        found.sort_by_key(|&(index, _, distance)| (distance, index));
        found
            .into_iter()
            .map(|(_, name, distance)| (name, distance))
            .collect()
    }
}
