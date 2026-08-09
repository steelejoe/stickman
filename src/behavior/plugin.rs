//! Behavior trait and BehaviorManager.

use crate::stickman::geometry::StickmanState;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;

/// Identifies each behavior for transitions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BehaviorId {
    Walking,
    Idle,
}

/// Context passed to behaviors during update.
pub struct UpdateContext<'a> {
    pub delta_ms: u64,
    pub display_width: u32,
    pub display_height: u32,
    pub stickman_state: &'a mut StickmanState,
}

/// Plugin behavior that controls stickman animation.
pub trait Behavior {
    fn id(&self) -> BehaviorId;
    fn update(&mut self, ctx: &mut UpdateContext) -> Option<BehaviorId>;
    fn draw<D>(&self, display: &mut D, state: &StickmanState) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>;
}

/// Manages current behavior and dispatches update/draw.
pub struct BehaviorManager {
    current: BehaviorVariant,
    order: [BehaviorId; 2],
    index: usize,
}

enum BehaviorVariant {
    Walking(crate::behavior::walking::WalkingBehavior),
    Idle(crate::behavior::idle::IdleBehavior),
}

impl BehaviorManager {
    pub fn new(_initial_state: StickmanState) -> Self {
        Self {
            current: BehaviorVariant::Walking(crate::behavior::walking::WalkingBehavior::new()),
            order: [BehaviorId::Walking, BehaviorId::Idle],
            index: 0,
        }
    }

    pub fn cycle_next(&mut self) {
        self.index = (self.index + 1) % self.order.len();
        let next_id = self.order[self.index];
        self.switch_to(next_id);
    }

    fn switch_to(&mut self, id: BehaviorId) {
        self.current = match id {
            BehaviorId::Walking => {
                BehaviorVariant::Walking(crate::behavior::walking::WalkingBehavior::new())
            }
            BehaviorId::Idle => BehaviorVariant::Idle(crate::behavior::idle::IdleBehavior::new()),
        };
    }

    pub fn update(&mut self, delta_ms: u64, state: &mut StickmanState) {
        let mut ctx = UpdateContext {
            delta_ms,
            display_width: crate::DISPLAY_WIDTH,
            display_height: crate::DISPLAY_HEIGHT,
            stickman_state: state,
        };
        let transition = match &mut self.current {
            BehaviorVariant::Walking(b) => b.update(&mut ctx),
            BehaviorVariant::Idle(b) => b.update(&mut ctx),
        };
        if let Some(next_id) = transition {
            self.switch_to(next_id);
        }
    }

    pub fn draw<D>(&self, display: &mut D, state: &StickmanState) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        match &self.current {
            BehaviorVariant::Walking(b) => b.draw(display, state),
            BehaviorVariant::Idle(b) => b.draw(display, state),
        }
    }
}
