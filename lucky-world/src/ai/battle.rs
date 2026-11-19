//! Battles: the war phase's hook (docs/AI.md, section 9). A brain's battle
//! layer will read a `BattleView` (only what its scouts and spies can see of
//! a front) and give `BattleOrder`s. Until there is war, the planner gives
//! none. (The fields and orders are read only once there is war.)
#![allow(dead_code)]

/// What a brain may see of a war: its own fronts, and others' armies as its
/// secret service estimates them. Empty until the war phase.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct BattleView {
    pub nation: usize,
    /// (front province, own strength there, estimated enemy strength there).
    pub fronts: Vec<(usize, f64, f64)>,
}

/// An order for an army.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum BattleOrder {
    Hold { province: usize },
    Attack { from: usize, to: usize },
    Retreat { from: usize, to: usize },
}

/// The battle layer of a brain.
pub trait BattlePlanner {
    fn plan(&self, view: &BattleView) -> Vec<BattleOrder>;
}

/// The planner until the war phase: no orders.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct NoBattles;

impl BattlePlanner for NoBattles {
    fn plan(&self, _view: &BattleView) -> Vec<BattleOrder> {
        Vec::new()
    }
}
