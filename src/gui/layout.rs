use gui::geom::{Box3, Pt3};

pub use cassowary::strength::{MEDIUM, REQUIRED, STRONG, WEAK};
use cassowary::WeightedRelation::*;
use cassowary::{Solver, Variable};

use std::collections::HashMap;

pub struct Layout {
    bounds: Box3,
    bounds_id: NodeId,
    solver: Solver,
    nodes: HashMap<NodeId, Node>,
    vars: HashMap<Variable, (NodeId, Field)>,
    id_count: usize,
}

impl Layout {
    pub fn new(bounds: Box3) -> Layout {
        let mut layout = Layout {
            bounds,
            bounds_id: NodeId(0),
            solver: Solver::new(),
            nodes: HashMap::new(),
            vars: HashMap::new(),
            id_count: 0,
        };
        layout.bounds_id = layout.add_node_internal(layout.bounds);
        let bounds_node = layout.nodes.get(&layout.bounds_id).unwrap();
        for (var, _) in &bounds_node.vars() {
            layout.solver.add_edit_variable(*var, STRONG).unwrap();
        }
        layout.set_bounds(bounds);
        layout
    }

    /// Set the bounding box of the entire region to be laid out
    pub fn set_bounds(&mut self, bounds: Box3) {
        let bounds_node = self.nodes.get(&self.bounds_id).unwrap();
        self.solver.suggest_value(bounds_node.x, bounds.pos.x.into()).unwrap();
        self.solver.suggest_value(bounds_node.y, bounds.pos.y.into()).unwrap();
        self.solver.suggest_value(bounds_node.z, bounds.pos.z.into()).unwrap();
        self.solver.suggest_value(bounds_node.width, bounds.size.x.into()).unwrap();
        self.solver.suggest_value(bounds_node.height, bounds.size.y.into()).unwrap();
        self.solver.suggest_value(bounds_node.depth, bounds.size.z.into()).unwrap();
    }

    fn next_node_id(&mut self) -> NodeId {
        self.id_count += 1;
        NodeId(self.id_count)
    }

    pub fn add_nodes(&mut self, n: usize) -> Vec<NodeId> {
        (0..n).map(|_| self.add_node()).collect::<Vec<_>>()
    }
    pub fn add_node(&mut self) -> NodeId {
        let id = self.add_node_internal(Box3::default());
        // all nodes go inside the boundary
        self.insert_inside(self.bounds_id, &[id]);

        // prefer to occupy as much space as possible
        self.suggest(id, Field::Width, ::std::f64::INFINITY, WEAK);
        self.suggest(id, Field::Height, ::std::f64::INFINITY, WEAK);
        self.suggest(id, Field::Depth, ::std::f64::INFINITY, WEAK);

        id
    }
    fn add_node_internal(&mut self, bounds: Box3) -> NodeId {
        let id = self.next_node_id();
        let node = Node::new(id, bounds);
        for (var, field) in &node.vars() {
            self.vars.insert(*var, (id, *field));
        }
        self.nodes.insert(id, node);
        id
    }

    /// Query the bounds of a given node
    pub fn query(&mut self, node: NodeId) -> Box3 {
        self.update();
        let node = self.nodes.get(&node).unwrap();
        Box3::new(node.value.pos + node.margin, node.value.size - node.margin)
    }

    fn update(&mut self) {
        let changes = self.solver.fetch_changes();
        for (var, val) in changes {
            let (node_id, field) = self.vars.get(var).unwrap();
            let node = self.nodes.get_mut(node_id).unwrap();
            match field {
                Field::X => node.value.pos.x = *val as f32,
                Field::Y => node.value.pos.y = *val as f32,
                Field::Z => node.value.pos.z = *val as f32,
                Field::Width => node.value.size.x = *val as f32,
                Field::Height => node.value.size.y = *val as f32,
                Field::Depth => node.value.size.z = *val as f32,
            }
        }
    }

    pub fn set_margin(&mut self, node: NodeId, margin: Pt3) {
        let node = self.nodes.get_mut(&node).unwrap();
        node.margin = margin;
    }

    /// Stack nodes along a specified dimension
    pub fn stack(&mut self, axis: Axis, items: &[NodeId]) {
        for item_ids in items.windows(2) {
            let item_a = self.nodes.get(&item_ids[0]).unwrap();
            let item_b = self.nodes.get(&item_ids[1]).unwrap();
            match axis {
                Axis::X => self.solver.add_constraint(item_a.x + item_a.width |LE(REQUIRED)| item_b.x),
                Axis::Y => self.solver.add_constraint(item_a.y + item_a.height |LE(REQUIRED)| item_b.y),
                Axis::Z => self.solver.add_constraint(item_a.z + item_a.depth |LE(REQUIRED)| item_b.z),
            }.unwrap();
        }
    }
    /// Require that all `items` are bounded within `outer`
    pub fn insert_inside(&mut self, outer_id: NodeId, items: &[NodeId]) {
        let outer = self.nodes.get(&outer_id).unwrap();
        for item_id in items {
            let item = self.nodes.get(item_id).unwrap();
            self.solver
                .add_constraints(&[
                    item.x |GE(REQUIRED)| outer.x,
                    item.y |GE(REQUIRED)| outer.y,
                    item.z |GE(REQUIRED)| outer.z,
                    item.x + item.width |LE(REQUIRED)| outer.x + outer.width,
                    item.y + item.height |LE(REQUIRED)| outer.y + outer.height,
                    item.z + item.depth |LE(REQUIRED)| outer.z + outer.depth,
                ])
                .unwrap();
        }
    }
    /// Require that the size of a along a specific axis is ratio times b
    pub fn ratio(&mut self, axis: Axis, a: NodeId, b: NodeId, ratio: f64, strength: f64) {
        let node_a = self.nodes.get(&a).unwrap();
        let node_b = self.nodes.get(&b).unwrap();
        match axis {
            Axis::X => self.solver.add_constraint(node_a.width |EQ(strength)| ratio * node_b.width),
            Axis::Y => self.solver.add_constraint(node_a.height |EQ(strength)| ratio * node_b.height),
            Axis::Z => self.solver.add_constraint(node_a.depth |EQ(strength)| ratio * node_b.depth),
        }.unwrap();
    }

    /// Constrain field of all items as being equal
    pub fn equalize(&mut self, field: Field, items: &[NodeId], strength: f64) {
        for item_ids in items.windows(2) {
            let item_a = self.nodes.get(&item_ids[0]).unwrap();
            let item_b = self.nodes.get(&item_ids[1]).unwrap();
            self.solver.add_constraint(item_a.get_var(field) |EQ(strength)| item_b.get_var(field)).unwrap();
        }
    }

    /// Suggest a value for a specific field of the given node
    pub fn suggest(&mut self, id: NodeId, field: Field, value: f64, strength: f64) {
        let node = self.nodes.get(&id).unwrap();
        self.solver.add_constraint(node.get_var(field) |EQ(strength)| value).unwrap();
    }
}

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub enum Axis {
    X,
    Y,
    Z,
}

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub enum Field {
    X,
    Y,
    Z,
    Width,
    Height,
    Depth,
}

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub struct NodeId(usize);

pub struct Node {
    id: NodeId,
    value: Box3,
    margin: Pt3,

    x: Variable,
    y: Variable,
    z: Variable,
    width: Variable,
    height: Variable,
    depth: Variable,
}

impl Node {
    fn new(id: NodeId, bounds: Box3) -> Node {
        Node {
            id,
            value: bounds,
            margin: Pt3::default(),
            x: Variable::new(),
            y: Variable::new(),
            z: Variable::new(),
            width: Variable::new(),
            height: Variable::new(),
            depth: Variable::new(),
        }
    }
    fn vars(&self) -> [(Variable, Field); 6] {
        [
            (self.x, Field::X),
            (self.y, Field::Y),
            (self.z, Field::Z),
            (self.width, Field::Width),
            (self.height, Field::Height),
            (self.depth, Field::Depth),
        ]
    }
    fn get_var(&self, field: Field) -> Variable {
        match field {
            Field::X => self.x,
            Field::Y => self.y,
            Field::Z => self.z,
            Field::Width => self.width,
            Field::Height => self.height,
            Field::Depth => self.depth,
        }
    }
}

#[test]
fn test_layout_basic() {
    let bounds = Box3::new(0.0.into(), 100.0.into());
    let mut layout = Layout::new(bounds);
    let node = layout.add_node();
    assert_eq!(layout.query(layout.bounds_id), layout.query(node));
    layout.set_bounds(Box3::new(25.0.into(), 75.0.into()));
    assert_eq!(layout.query(layout.bounds_id), layout.query(node));
    let node2 = layout.add_node();
    assert_eq!(layout.query(layout.bounds_id), layout.query(node2));
    layout.stack(Axis::X, &[node, node2]);
    layout.ratio(Axis::X, node, node2, 1.0, REQUIRED);
    layout.suggest(node, Field::Width, 100.0, WEAK);
    assert_eq!(layout.query(node).size, layout.query(node2).size);
}

#[test]
fn test_layout_ratio() {
    let bounds = Box3::new(0.0.into(), 100.0.into());
    let mut layout = Layout::new(bounds);
    let node = layout.add_node();
    let node2 = layout.add_node();
    layout.stack(Axis::X, &[node, node2]);
    layout.stack(Axis::Y, &[node, node2]);
    layout.ratio(Axis::X, node, node2, 2.0, REQUIRED);
    layout.ratio(Axis::Y, node, node2, 3.0, REQUIRED);
    let b1 = layout.query(node);
    let b2 = layout.query(node2);
    assert_eq!(b1.size.x, 2.0 * b2.size.x);
    assert_eq!(b1.size.y, 3.0 * b2.size.y);
    assert_eq!(b1.pos.x + b1.size.x, b2.pos.x);
    assert_eq!(b1.pos.y + b1.size.y, b2.pos.y);
}
