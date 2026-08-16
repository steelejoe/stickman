//! Stickman species and clips. Authoring is hand-written IR (no parser).

use crate::stickman::ir::{
    Bone, BoneKind, Clip, ClipId, Interp, Key, LoopMode, Prop, Species, Spin, Track,
};

/// Walk / tumble / knockback loop length (100 gait units at 90 units/s).
pub const WALK_MS: u16 = 1111;
/// Pixels of logical X per walk/tumble loop (knockback uses the negation).
const TRAVEL_DX: i16 = 60;
const STAB_MS: u16 = 900;
const JUMP_MS: u16 = 750;

pub const ROOT: u8 = 0;
pub const HIP: u8 = 1;
pub const SPINE: u8 = 2;
pub const NECK: u8 = 3;
pub const HEAD: u8 = 4;
pub const THIGH_A: u8 = 5;
pub const SHIN_A: u8 = 6;
pub const THIGH_B: u8 = 7;
pub const SHIN_B: u8 = 8;
pub const ARM_A: u8 = 9;
pub const FOREARM_A: u8 = 10;
pub const ARM_B: u8 = 11;
pub const FOREARM_B: u8 = 12;
pub const FIST: u8 = 13;
pub const SWORD: u8 = 14;
pub const GUARD: u8 = 15;

const LINE: BoneKind = BoneKind::Line;
const JOINT: BoneKind = BoneKind::Joint;

/// Shared stickman rig. Sword bones exist but rest hidden.
pub static STICKMAN: Species = Species {
    bones: &[
        Bone {
            parent: -1,
            length: 0,
            rest_deg: 0,
            kind: JOINT,
            visible: false,
        },
        // hip: feet → hip joint (no stroke; legs are the support)
        Bone {
            parent: 0,
            length: 28,
            rest_deg: 180,
            kind: JOINT,
            visible: false,
        },
        Bone {
            parent: 1,
            length: 18,
            rest_deg: 180,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 2,
            length: 6,
            rest_deg: 180,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 3,
            length: 6,
            rest_deg: 180,
            kind: BoneKind::Circle { diameter: 12 },
            visible: true,
        },
        Bone {
            parent: 1,
            length: 15,
            rest_deg: 0,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 5,
            length: 13,
            rest_deg: -8,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 1,
            length: 15,
            rest_deg: 0,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 7,
            length: 13,
            rest_deg: -12,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 2,
            length: 12,
            rest_deg: 0,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 9,
            length: 11,
            rest_deg: 18,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 2,
            length: 12,
            rest_deg: 0,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 11,
            length: 11,
            rest_deg: 18,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 10,
            length: 0,
            rest_deg: 0,
            kind: BoneKind::Circle { diameter: 4 },
            visible: false,
        },
        Bone {
            parent: 13,
            length: 24,
            rest_deg: 90,
            kind: LINE,
            visible: false,
        },
        Bone {
            parent: 13,
            length: 6,
            rest_deg: 0,
            kind: LINE,
            visible: false,
        },
    ],
};

macro_rules! track {
    ($bone:expr, $prop:ident, $(($t:expr, $v:expr, $i:ident)),+ $(,)?) => {
        Track {
            bone: $bone,
            prop: Prop::$prop,
            keys: &[
                $(Key {
                    t_ms: $t,
                    value: $v,
                    interp: Interp::$i,
                }),+
            ],
        }
    };
}

const HIP_CROUCH: Track = track!(HIP, Len, (0, 12, Hold));
const SPINE_150: Track = track!(SPINE, Rot, (0, 150, Hold));
const NECK_150: Track = track!(NECK, Rot, (0, 150, Hold));
const HEAD_150: Track = track!(HEAD, Rot, (0, 150, Hold));
const SPINE_180: Track = track!(SPINE, Rot, (0, 180, Hold));
const NECK_180: Track = track!(NECK, Rot, (0, 180, Hold));
const HEAD_180: Track = track!(HEAD, Rot, (0, 180, Hold));
const CROUCH_THIGH_A: Track = track!(THIGH_A, Rot, (0, 22, Hold));
const CROUCH_SHIN_A: Track = track!(SHIN_A, Rot, (0, -46, Hold));
const CROUCH_THIGH_B: Track = track!(THIGH_B, Rot, (0, -11, Hold));
const CROUCH_SHIN_B: Track = track!(SHIN_B, Rot, (0, -83, Hold));
const FIST_ON: Track = track!(FIST, Visible, (0, 1, Hold));
const SWORD_ON: Track = track!(SWORD, Visible, (0, 1, Hold));
const GUARD_ON: Track = track!(GUARD, Visible, (0, 1, Hold));
const SWORD_ARM_A: Track = track!(ARM_A, Rot, (0, 32, Hold));
const SWORD_FOREARM_A: Track = track!(FOREARM_A, Rot, (0, 82, Hold));
const SWORD_ARM_B: Track = track!(ARM_B, Rot, (0, 18, Hold));
const SWORD_FOREARM_B: Track = track!(FOREARM_B, Rot, (0, 80, Hold));
const SWORD_ROT: Track = track!(SWORD, Rot, (0, 106, Hold));

static IDLE: Clip = Clip {
    species: &STICKMAN,
    duration_ms: 1,
    loop_mode: LoopMode::Once,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[],
};

/// Gait keys at 0 / 25 / 50 / 75% of [`WALK_MS`].
static WALK: Clip = Clip {
    species: &STICKMAN,
    duration_ms: WALK_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: TRAVEL_DX,
    spin: Spin::None,
    tracks: &[
        track!(
            THIGH_A,
            Rot,
            (0, 0, Lerp),
            (278, 32, Lerp),
            (556, 0, Lerp),
            (833, -32, Lerp)
        ),
        track!(
            SHIN_A,
            Rot,
            (0, -8, Lerp),
            (278, 18, Lerp),
            (556, -12, Lerp),
            (833, -92, Lerp)
        ),
        track!(
            THIGH_B,
            Rot,
            (0, 0, Lerp),
            (278, -32, Lerp),
            (556, 0, Lerp),
            (833, 32, Lerp)
        ),
        track!(
            SHIN_B,
            Rot,
            (0, -12, Lerp),
            (278, -92, Lerp),
            (556, -8, Lerp),
            (833, 18, Lerp)
        ),
        track!(
            ARM_A,
            Rot,
            (0, 0, Lerp),
            (278, -28, Lerp),
            (556, 0, Lerp),
            (833, 28, Lerp)
        ),
        track!(
            FOREARM_A,
            Rot,
            (0, 18, Lerp),
            (278, 4, Lerp),
            (556, 18, Lerp),
            (833, 60, Lerp)
        ),
        track!(
            ARM_B,
            Rot,
            (0, 0, Lerp),
            (278, 28, Lerp),
            (556, 0, Lerp),
            (833, -28, Lerp)
        ),
        track!(
            FOREARM_B,
            Rot,
            (0, 18, Lerp),
            (278, 60, Lerp),
            (556, 18, Lerp),
            (833, 4, Lerp)
        ),
    ],
};

static JUMP: Clip = Clip {
    species: &STICKMAN,
    duration_ms: JUMP_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[
        track!(THIGH_A, Rot, (0, 18, Hold)),
        track!(SHIN_A, Rot, (0, -22, Hold)),
        track!(THIGH_B, Rot, (0, -14, Hold)),
        track!(SHIN_B, Rot, (0, -50, Hold)),
        track!(ARM_A, Rot, (0, -150, Hold)),
        track!(FOREARM_A, Rot, (0, -125, Hold)),
        track!(ARM_B, Rot, (0, -135, Hold)),
        track!(FOREARM_B, Rot, (0, -105, Hold)),
    ],
};

static CROUCH: Clip = Clip {
    species: &STICKMAN,
    duration_ms: 1,
    loop_mode: LoopMode::Once,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[
        HIP_CROUCH,
        SPINE_150,
        NECK_150,
        HEAD_150,
        CROUCH_THIGH_A,
        CROUCH_SHIN_A,
        CROUCH_THIGH_B,
        CROUCH_SHIN_B,
        track!(ARM_A, Rot, (0, 10, Hold)),
        track!(FOREARM_A, Rot, (0, 28, Hold)),
        track!(ARM_B, Rot, (0, -8, Hold)),
        track!(FOREARM_B, Rot, (0, 14, Hold)),
    ],
};

static BEG: Clip = Clip {
    species: &STICKMAN,
    duration_ms: 1,
    loop_mode: LoopMode::Once,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[
        HIP_CROUCH,
        SPINE_180,
        NECK_180,
        HEAD_180,
        CROUCH_THIGH_A,
        CROUCH_SHIN_A,
        CROUCH_THIGH_B,
        CROUCH_SHIN_B,
        track!(ARM_A, Rot, (0, 70, Hold)),
        track!(FOREARM_A, Rot, (0, 130, Hold)),
        track!(ARM_B, Rot, (0, 62, Hold)),
        track!(FOREARM_B, Rot, (0, 128, Hold)),
    ],
};

static SWORD_STANCE: Clip = Clip {
    species: &STICKMAN,
    duration_ms: 1,
    loop_mode: LoopMode::Once,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[
        track!(THIGH_A, Rot, (0, 10, Hold)),
        track!(SHIN_A, Rot, (0, 0, Hold)),
        track!(THIGH_B, Rot, (0, -8, Hold)),
        track!(SHIN_B, Rot, (0, -20, Hold)),
        SWORD_ARM_A,
        SWORD_FOREARM_A,
        SWORD_ARM_B,
        SWORD_FOREARM_B,
        FIST_ON,
        SWORD_ON,
        GUARD_ON,
        SWORD_ROT,
    ],
};

static SWORD_STAB: Clip = Clip {
    species: &STICKMAN,
    duration_ms: STAB_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[
        track!(ROOT, Tx, (0, 0, Lerp), (450, 10, Lerp), (900, 0, Lerp)),
        track!(
            THIGH_A,
            Rot,
            (0, 10, Lerp),
            (450, 38, Lerp),
            (900, 10, Lerp)
        ),
        track!(SHIN_A, Rot, (0, 0, Lerp), (450, 18, Lerp), (900, 0, Lerp)),
        track!(
            THIGH_B,
            Rot,
            (0, -8, Lerp),
            (450, -26, Lerp),
            (900, -8, Lerp)
        ),
        track!(
            SHIN_B,
            Rot,
            (0, -20, Lerp),
            (450, -46, Lerp),
            (900, -20, Lerp)
        ),
        track!(
            SPINE,
            Rot,
            (0, 180, Lerp),
            (450, 162, Lerp),
            (900, 180, Lerp)
        ),
        track!(
            NECK,
            Rot,
            (0, 180, Lerp),
            (450, 162, Lerp),
            (900, 180, Lerp)
        ),
        track!(
            HEAD,
            Rot,
            (0, 180, Lerp),
            (450, 162, Lerp),
            (900, 180, Lerp)
        ),
        track!(ARM_A, Rot, (0, 32, Lerp), (450, 90, Lerp), (900, 32, Lerp)),
        track!(
            FOREARM_A,
            Rot,
            (0, 82, Lerp),
            (450, 90, Lerp),
            (900, 82, Lerp)
        ),
        SWORD_ARM_B,
        SWORD_FOREARM_B,
        FIST_ON,
        SWORD_ON,
        GUARD_ON,
        track!(
            SWORD,
            Rot,
            (0, 106, Lerp),
            (450, 90, Lerp),
            (900, 106, Lerp)
        ),
    ],
};

static SWORD_CROUCH_STANCE: Clip = Clip {
    species: &STICKMAN,
    duration_ms: 1,
    loop_mode: LoopMode::Once,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[
        HIP_CROUCH,
        SPINE_150,
        NECK_150,
        HEAD_150,
        track!(THIGH_A, Rot, (0, 58, Hold)),
        track!(SHIN_A, Rot, (0, -14, Hold)),
        track!(THIGH_B, Rot, (0, -28, Hold)),
        track!(SHIN_B, Rot, (0, -126, Hold)),
        SWORD_ARM_A,
        SWORD_FOREARM_A,
        SWORD_ARM_B,
        SWORD_FOREARM_B,
        FIST_ON,
        SWORD_ON,
        GUARD_ON,
        SWORD_ROT,
    ],
};

static SWORD_CROUCH_STAB: Clip = Clip {
    species: &STICKMAN,
    duration_ms: STAB_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[
        track!(ROOT, Tx, (0, 0, Lerp), (450, 10, Lerp), (900, 0, Lerp)),
        HIP_CROUCH,
        track!(
            SPINE,
            Rot,
            (0, 150, Lerp),
            (450, 138, Lerp),
            (900, 150, Lerp)
        ),
        track!(
            NECK,
            Rot,
            (0, 150, Lerp),
            (450, 138, Lerp),
            (900, 150, Lerp)
        ),
        track!(
            HEAD,
            Rot,
            (0, 150, Lerp),
            (450, 138, Lerp),
            (900, 150, Lerp)
        ),
        track!(
            THIGH_A,
            Rot,
            (0, 58, Lerp),
            (450, 80, Lerp),
            (900, 58, Lerp)
        ),
        track!(
            SHIN_A,
            Rot,
            (0, -14, Lerp),
            (450, 2, Lerp),
            (900, -14, Lerp)
        ),
        track!(
            THIGH_B,
            Rot,
            (0, -28, Lerp),
            (450, -38, Lerp),
            (900, -28, Lerp)
        ),
        track!(
            SHIN_B,
            Rot,
            (0, -126, Lerp),
            (450, -136, Lerp),
            (900, -126, Lerp)
        ),
        track!(ARM_A, Rot, (0, 32, Lerp), (450, 90, Lerp), (900, 32, Lerp)),
        track!(
            FOREARM_A,
            Rot,
            (0, 82, Lerp),
            (450, 90, Lerp),
            (900, 82, Lerp)
        ),
        SWORD_ARM_B,
        SWORD_FOREARM_B,
        FIST_ON,
        SWORD_ON,
        GUARD_ON,
        track!(
            SWORD,
            Rot,
            (0, 106, Lerp),
            (450, 90, Lerp),
            (900, 106, Lerp)
        ),
    ],
};

static KNOCKBACK: Clip = Clip {
    species: &STICKMAN,
    duration_ms: WALK_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: -TRAVEL_DX,
    spin: Spin::Knockback,
    tracks: &[
        track!(THIGH_A, Rot, (0, 50, Hold)),
        track!(SHIN_A, Rot, (0, -30, Hold)),
        track!(THIGH_B, Rot, (0, -40, Hold)),
        track!(SHIN_B, Rot, (0, -120, Hold)),
        track!(ARM_A, Rot, (0, 80, Hold)),
        track!(FOREARM_A, Rot, (0, 100, Hold)),
        track!(ARM_B, Rot, (0, -70, Hold)),
        track!(FOREARM_B, Rot, (0, -50, Hold)),
        track!(ROOT, Spin, (0, 0, Lerp), (WALK_MS, 360, Lerp)),
    ],
};

static TUMBLE: Clip = Clip {
    species: &STICKMAN,
    duration_ms: WALK_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: TRAVEL_DX,
    spin: Spin::Tumble,
    tracks: &[
        track!(HIP, Len, (0, 18, Hold)),
        SPINE_150,
        NECK_150,
        HEAD_150,
        track!(THIGH_A, Rot, (0, 58, Hold)),
        track!(SHIN_A, Rot, (0, -14, Hold)),
        track!(THIGH_B, Rot, (0, -28, Hold)),
        track!(SHIN_B, Rot, (0, -126, Hold)),
        track!(ARM_A, Rot, (0, 50, Hold)),
        track!(FOREARM_A, Rot, (0, 90, Hold)),
        track!(ARM_B, Rot, (0, 40, Hold)),
        track!(FOREARM_B, Rot, (0, 80, Hold)),
        track!(ROOT, Spin, (0, 0, Lerp), (WALK_MS, 360, Lerp)),
    ],
};

/// Look up clip data. Searching reuses the crouch pose.
pub fn clip(id: ClipId) -> &'static Clip {
    match id {
        ClipId::Walk => &WALK,
        ClipId::Idle => &IDLE,
        ClipId::Jump => &JUMP,
        ClipId::Crouch => &CROUCH,
        ClipId::Beg => &BEG,
        ClipId::SwordStance => &SWORD_STANCE,
        ClipId::SwordStab => &SWORD_STAB,
        ClipId::SwordCrouchStance => &SWORD_CROUCH_STANCE,
        ClipId::SwordCrouchStab => &SWORD_CROUCH_STAB,
        ClipId::Knockback => &KNOCKBACK,
        ClipId::Tumble => &TUMBLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_travel_is_shared() {
        assert_eq!(WALK.travel_dx, TRAVEL_DX);
        assert_eq!(TUMBLE.travel_dx, TRAVEL_DX);
        assert_eq!(KNOCKBACK.travel_dx, -TRAVEL_DX);
    }

    #[test]
    fn stickman_fits_scratch() {
        assert!(STICKMAN.bones.len() <= crate::stickman::ir::MAX_BONES);
        assert_eq!(STICKMAN.bones.len(), 16);
    }
}
