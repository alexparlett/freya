use freya_core::{
    helpers::from_fn_standalone_borrowed_keyed,
    integration::*,
    path_element::PathElement,
    prelude::*,
};

#[derive(PartialEq, Clone)]
struct Name(u8);

impl Component for Name {
    fn render(&self) -> impl IntoElement {
        label().text(format!("{}", self.0))
    }
}

/// Fails if its scope is ever handed another row's props.
#[derive(PartialEq, Clone)]
struct Keeper(u8);

impl Component for Keeper {
    fn render(&self) -> impl IntoElement {
        let born = use_hook(|| self.0);
        assert_eq!(
            born, self.0,
            "row {born}'s component was given row {}'s props",
            self.0
        );
        rect()
    }
}

/// A keyed row whose first child holds a component only for even keys when `shaped`, as a
/// folder's disclosure holds a glyph and a file's does not.
fn row(key: u8, shaped: bool) -> Element {
    rect()
        .key(key)
        .child(rect().maybe(shaped && key.is_multiple_of(2), |el| el.child(Name(key))))
        .child(Keeper(key))
        .into_element()
}

fn replay(history: &[Vec<u8>], shaped: bool) {
    fn collect(element: &PathElement, output: &mut Vec<Vec<u32>>) {
        match element {
            PathElement::Component { path, .. } => output.push(path.to_vec()),
            PathElement::Element { path, elements, .. } => {
                output.push(path.to_vec());
                for child in elements.iter() {
                    collect(child, output);
                }
            }
        }
    }

    fn app() -> Element {
        let (order, shaped) = consume_context::<State<(Vec<u8>, bool)>>().read().clone();
        rect()
            .children(order.into_iter().map(|k| row(k, shaped)))
            .into()
    }

    let mut runner = Runner::new(app);
    let mut tree = Tree::default();
    let mut state = runner.provide_root_context(|| State::create((Vec::<u8>::new(), shaped)));
    let mutations = runner.sync_and_update();
    tree.apply_mutations(mutations, 1.0);

    for step in history {
        state.set((step.clone(), shaped));
        let mutations = runner.sync_and_update();
        tree.apply_mutations(mutations, 1.0);
        tree.verify_tree_integrity();
        assert_eq!(
            tree.elements.len(),
            runner.node_to_scope.len(),
            "after {step:?}"
        );
        for scope_rc in runner.scopes.values() {
            let scope = scope_rc.borrow();
            let mut element_paths = Vec::new();
            if let Some(element) = scope.element.as_ref() {
                collect(element, &mut element_paths);
            }
            element_paths.sort();
            let mut node_paths = Vec::new();
            scope.nodes.traverse(&[], |path, _| {
                if !path.is_empty() {
                    node_paths.push(path.to_vec());
                }
            });
            node_paths.sort();
            assert_eq!(
                element_paths, node_paths,
                "scope {:?} diverged after {step:?}",
                scope.id
            );
        }
        // The tree's rows are in the order of the scope's nodes.
        let root = runner.scopes.get(&ScopeId::ROOT).unwrap().borrow();
        let expected: Vec<NodeId> = (0..step.len() as u32)
            .map(|i| root.nodes.get(&[0, i]).unwrap().node_id)
            .collect();
        let list = root.nodes.get(&[0]).unwrap().node_id;
        assert_eq!(
            tree.children.get(&list).cloned().unwrap_or_default(),
            expected,
            "tree order after {step:?}"
        );
    }
}

fn permutations(keys: &[u8]) -> Vec<Vec<u8>> {
    if keys.is_empty() {
        return vec![Vec::new()];
    }
    let mut output = Vec::new();
    for index in 0..keys.len() {
        let mut rest = keys.to_vec();
        let first = rest.remove(index);
        for mut p in permutations(&rest) {
            p.insert(0, first);
            output.push(p);
        }
    }
    output
}

/// An unmoved row between moved ones that are not placed yet.
#[test]
fn unmoved_row_between_moved_rows() {
    replay(&[vec![0, 1, 2, 3, 4], vec![2, 1, 3, 4, 0]], true);
    replay(&[vec![0, 1, 2, 3], vec![2, 1, 3, 0]], false);
}

/// The orders Isle Files' list went through when sorted by date, its two folders shaped
/// apart from its files.
#[test]
fn files_rows_sorted_by_date() {
    let by_name = vec![0, 6, 1, 2, 3, 4, 5, 7, 8, 9];
    let by_created = vec![3, 5, 4, 7, 0, 6, 9, 2, 8, 1];
    let by_modified = vec![5, 3, 7, 4, 0, 6, 1, 9, 2, 8];
    replay(
        &[
            by_name.clone(),
            by_created.clone(),
            by_name,
            by_modified,
            by_created,
        ],
        true,
    );
}

#[test]
fn every_reorder_of_five() {
    let base = vec![0, 1, 2, 3, 4];
    for shaped in [false, true] {
        let mut history = Vec::new();
        for to in permutations(&base) {
            history.push(base.clone());
            history.push(to);
        }
        replay(&history, shaped);
    }
}

#[test]
fn reorders_with_additions_and_removals() {
    let layouts: Vec<Vec<u8>> = (0..=4)
        .flat_map(|n| permutations(&(0..n).collect::<Vec<u8>>()))
        .collect();
    for shaped in [false, true] {
        let mut history = Vec::new();
        for from in &layouts {
            for to in &layouts {
                history.push(from.clone());
                history.push(to.clone());
            }
        }
        replay(&history, shaped);
    }
}

fn keeper_row(key: &u8) -> Element {
    let born = use_hook(|| *key);
    assert_eq!(born, *key, "row {born} was given row {key}'s props");
    rect().child(Keeper(*key)).into()
}

/// Rows that are keyed components themselves, moved with additions and removals.
#[test]
fn component_rows_reorder() {
    fn app() -> Element {
        let order = consume_context::<State<Vec<u8>>>().read().clone();
        rect()
            .children(
                order
                    .into_iter()
                    .map(|k| from_fn_standalone_borrowed_keyed(k, k, keeper_row)),
            )
            .into()
    }
    let layouts: Vec<Vec<u8>> = (0..=4)
        .flat_map(|n| permutations(&(0..n).collect::<Vec<u8>>()))
        .collect();
    let mut runner = Runner::new(app);
    let mut tree = Tree::default();
    let mut state = runner.provide_root_context(|| State::create(Vec::<u8>::new()));
    let mutations = runner.sync_and_update();
    tree.apply_mutations(mutations, 1.0);
    for from in &layouts {
        for to in &layouts {
            for step in [from, to] {
                state.set(step.clone());
                let mutations = runner.sync_and_update();
                tree.apply_mutations(mutations, 1.0);
                tree.verify_tree_integrity();
                assert_eq!(
                    tree.elements.len(),
                    runner.node_to_scope.len(),
                    "after {step:?}"
                );
                let root = runner.scopes.get(&ScopeId::ROOT).unwrap().borrow();
                let list = root.nodes.get(&[0]).unwrap().node_id;
                // Each row's root in the tree belongs to the scope at that index.
                let expected: Vec<NodeId> = (0..step.len() as u32)
                    .map(|i| {
                        let scope_id = root.nodes.get(&[0, i]).unwrap().scope_id.unwrap();
                        runner
                            .scopes
                            .get(&scope_id)
                            .unwrap()
                            .borrow()
                            .nodes
                            .get(&[0])
                            .unwrap()
                            .node_id
                    })
                    .collect();
                assert_eq!(
                    tree.children.get(&list).cloned().unwrap_or_default(),
                    expected,
                    "tree order after {from:?} -> {step:?}"
                );
            }
        }
    }
}
