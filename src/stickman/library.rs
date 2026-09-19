//! Species and clips. Authoring is hand-written IR (no parser).

use crate::stickman::geometry::STANDING_HEIGHT;
use crate::stickman::ir::{
    Bone, BoneKind, Clip, ClipId, Interp, Key, LoopMode, Prop, Species, Spin, Track,
};

/// Crate height: half the standing stickman (feet → top of head).
pub const BOX_HEIGHT: u32 = (STANDING_HEIGHT / 2) as u32;
/// Square footprint matching [`BOX_HEIGHT`].
pub const BOX_WIDTH: u32 = BOX_HEIGHT;

/// Walk / tumble / knockback loop length (100 gait units at 90 units/s).
pub const WALK_MS: u16 = 1111;
/// Pixels of logical X per walk/tumble loop (knockback uses the negation).
const TRAVEL_DX: i16 = 60;
/// Shorter stride than walk; same loop length so crawl keys share the gait clock.
const CRAWL_DX: i16 = 36;
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
/// Dog tail (same slot as [`FIST`]; plant-feet still ignores index ≥ 13).
pub const TAIL: u8 = 13;
pub const EAR_A: u8 = 14;
pub const EAR_B: u8 = 15;

/// Horizontal torso on the dog rig. ~33% shorter than the long sketch body.
pub const DOG_SPINE: i16 = 24;
/// Ground → rear hip. Front legs hang the same distance from the shoulder.
pub const DOG_HIP: i16 = 20;

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

/// Crate on the floor edge. One rect bone (spiral fill); motion lives in the clips.
pub static BOX: Species = Species {
    bones: &[Bone {
        parent: -1,
        length: 0,
        rest_deg: 0,
        kind: BoneKind::Rect {
            width: BOX_WIDTH,
            height: BOX_HEIGHT,
        },
        visible: true,
    }],
};

/// Side-profile quadruped: line limbs, ellipse snout, triangle ears.
pub static DOG: Species = Species {
    bones: &[
        Bone {
            parent: -1,
            length: 0,
            rest_deg: 0,
            kind: JOINT,
            visible: false,
        },
        Bone {
            parent: 0,
            length: DOG_HIP,
            rest_deg: 180,
            kind: JOINT,
            visible: false,
        },
        Bone {
            parent: 1,
            length: DOG_SPINE,
            rest_deg: 90,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 2,
            length: 5,
            rest_deg: 155,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 3,
            length: 6,
            rest_deg: 135,
            kind: BoneKind::Ellipse {
                width: 20,
                height: 9,
            },
            visible: true,
        },
        Bone {
            parent: 1,
            length: 10,
            rest_deg: 12,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 5,
            length: 10,
            rest_deg: -12,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 1,
            length: 10,
            rest_deg: -14,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 7,
            length: 10,
            rest_deg: -16,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 2,
            length: 10,
            rest_deg: 10,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 9,
            length: 10,
            rest_deg: -10,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 2,
            length: 10,
            rest_deg: -16,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 11,
            length: 10,
            rest_deg: -14,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 1,
            length: 12,
            rest_deg: 220,
            kind: LINE,
            visible: true,
        },
        Bone {
            parent: 4,
            length: 11,
            rest_deg: 205,
            kind: BoneKind::Triangle { base: 6 },
            visible: true,
        },
        Bone {
            parent: 4,
            length: 11,
            rest_deg: 155,
            kind: BoneKind::Triangle { base: 6 },
            visible: true,
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
/// All-fours lean (more horizontal than crouch's 150°).
const SPINE_102: Track = track!(SPINE, Rot, (0, 102, Hold));
const NECK_124: Track = track!(NECK, Rot, (0, 124, Hold));
const HEAD_148: Track = track!(HEAD, Rot, (0, 148, Hold));
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

const JUMP_TRACKS: &[Track] = &[
    track!(THIGH_A, Rot, (0, 18, Hold)),
    track!(SHIN_A, Rot, (0, -22, Hold)),
    track!(THIGH_B, Rot, (0, -14, Hold)),
    track!(SHIN_B, Rot, (0, -50, Hold)),
    track!(ARM_A, Rot, (0, -150, Hold)),
    track!(FOREARM_A, Rot, (0, -125, Hold)),
    track!(ARM_B, Rot, (0, -135, Hold)),
    track!(FOREARM_B, Rot, (0, -105, Hold)),
];

static JUMP: Clip = Clip {
    species: &STICKMAN,
    duration_ms: JUMP_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: 0,
    spin: Spin::None,
    tracks: JUMP_TRACKS,
};

static JUMP_FORWARD: Clip = Clip {
    species: &STICKMAN,
    duration_ms: JUMP_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: TRAVEL_DX,
    spin: Spin::None,
    tracks: JUMP_TRACKS,
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

/// Hands-and-knees from the crouch: low hip, torso flatter, contralateral gait.
static CRAWL: Clip = Clip {
    species: &STICKMAN,
    duration_ms: WALK_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: CRAWL_DX,
    spin: Spin::None,
    tracks: &[
        HIP_CROUCH,
        SPINE_102,
        NECK_124,
        HEAD_148,
        track!(
            THIGH_A,
            Rot,
            (0, 10, Lerp),
            (278, 28, Lerp),
            (556, 44, Lerp),
            (833, 28, Lerp)
        ),
        track!(
            SHIN_A,
            Rot,
            (0, -116, Lerp),
            (278, -108, Lerp),
            (556, -98, Lerp),
            (833, -108, Lerp)
        ),
        track!(
            THIGH_B,
            Rot,
            (0, 44, Lerp),
            (278, 28, Lerp),
            (556, 10, Lerp),
            (833, 28, Lerp)
        ),
        track!(
            SHIN_B,
            Rot,
            (0, -98, Lerp),
            (278, -108, Lerp),
            (556, -116, Lerp),
            (833, -108, Lerp)
        ),
        track!(
            ARM_A,
            Rot,
            (0, 62, Lerp),
            (278, 48, Lerp),
            (556, 34, Lerp),
            (833, 48, Lerp)
        ),
        track!(
            FOREARM_A,
            Rot,
            (0, 10, Lerp),
            (278, 8, Lerp),
            (556, 8, Lerp),
            (833, 8, Lerp)
        ),
        track!(
            ARM_B,
            Rot,
            (0, 34, Lerp),
            (278, 48, Lerp),
            (556, 62, Lerp),
            (833, 48, Lerp)
        ),
        track!(
            FOREARM_B,
            Rot,
            (0, 8, Lerp),
            (278, 8, Lerp),
            (556, 10, Lerp),
            (833, 8, Lerp)
        ),
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
    travel_dx: TRAVEL_DX,
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

static BOX_IDLE: Clip = Clip {
    species: &BOX,
    duration_ms: 1,
    loop_mode: LoopMode::Once,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[],
};

const BOX_SLIDE_MS: u16 = 800;
const BOX_SLIDE_DX: i16 = 40;
const BOX_ROLL_DX: i16 = 50;
const SHUDDER_MS: u16 = 400;
const FLIP_MS: u16 = 300;

/// Short looping turn; facing follows the reversed travel vector (or toggles
/// when standing still).
static FLIP: Clip = Clip {
    species: &STICKMAN,
    duration_ms: FLIP_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[track!(ROOT, Tx, (0, 0, Hold))],
};

static BOX_SLIDE: Clip = Clip {
    species: &BOX,
    duration_ms: BOX_SLIDE_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: BOX_SLIDE_DX,
    spin: Spin::None,
    tracks: &[],
};

static BOX_ROLL: Clip = Clip {
    species: &BOX,
    duration_ms: WALK_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: BOX_ROLL_DX,
    spin: Spin::Tumble,
    tracks: &[track!(ROOT, Spin, (0, 0, Lerp), (WALK_MS, 360, Lerp))],
};

static BOX_SHUDDER: Clip = Clip {
    species: &BOX,
    duration_ms: SHUDDER_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[track!(
        ROOT,
        Tx,
        (0, 0, Lerp),
        (80, -4, Lerp),
        (160, 4, Lerp),
        (240, -4, Lerp),
        (320, 4, Lerp),
        (400, 0, Lerp)
    )],
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

const DOG_HIP_CROUCH: Track = track!(HIP, Len, (0, 11, Hold));
const DOG_SPINE_90: Track = track!(SPINE, Rot, (0, 90, Hold));
const DOG_SPINE_80: Track = track!(SPINE, Rot, (0, 80, Hold));
const DOG_SPINE_140: Track = track!(SPINE, Rot, (0, 140, Hold));
const DOG_NECK_155: Track = track!(NECK, Rot, (0, 155, Hold));
const DOG_NECK_148: Track = track!(NECK, Rot, (0, 148, Hold));
const DOG_HEAD_135: Track = track!(HEAD, Rot, (0, 135, Hold));
const DOG_HEAD_128: Track = track!(HEAD, Rot, (0, 128, Hold));

static DOG_IDLE: Clip = Clip {
    species: &DOG,
    duration_ms: 1,
    loop_mode: LoopMode::Once,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[],
};

/// Trot: diagonal pairs (back A + front B, back B + front A).
static DOG_WALK: Clip = Clip {
    species: &DOG,
    duration_ms: WALK_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: TRAVEL_DX,
    spin: Spin::None,
    tracks: &[
        track!(
            THIGH_A,
            Rot,
            (0, 8, Lerp),
            (278, 32, Lerp),
            (556, 8, Lerp),
            (833, -22, Lerp)
        ),
        track!(
            SHIN_A,
            Rot,
            (0, -12, Lerp),
            (278, 8, Lerp),
            (556, -16, Lerp),
            (833, -70, Lerp)
        ),
        track!(
            THIGH_B,
            Rot,
            (0, -6, Lerp),
            (278, -22, Lerp),
            (556, -6, Lerp),
            (833, 28, Lerp)
        ),
        track!(
            SHIN_B,
            Rot,
            (0, -16, Lerp),
            (278, -70, Lerp),
            (556, -12, Lerp),
            (833, 8, Lerp)
        ),
        track!(
            ARM_A,
            Rot,
            (0, -8, Lerp),
            (278, -20, Lerp),
            (556, -8, Lerp),
            (833, 28, Lerp)
        ),
        track!(
            FOREARM_A,
            Rot,
            (0, -14, Lerp),
            (278, -68, Lerp),
            (556, -12, Lerp),
            (833, 6, Lerp)
        ),
        track!(
            ARM_B,
            Rot,
            (0, 6, Lerp),
            (278, 28, Lerp),
            (556, 6, Lerp),
            (833, -20, Lerp)
        ),
        track!(
            FOREARM_B,
            Rot,
            (0, -10, Lerp),
            (278, 6, Lerp),
            (556, -16, Lerp),
            (833, -68, Lerp)
        ),
        track!(
            TAIL,
            Rot,
            (0, 210, Lerp),
            (278, 235, Lerp),
            (556, 210, Lerp),
            (833, 235, Lerp)
        ),
    ],
};

const DOG_JUMP_TRACKS: &[Track] = &[
    track!(THIGH_A, Rot, (0, 28, Hold)),
    track!(SHIN_A, Rot, (0, -70, Hold)),
    track!(THIGH_B, Rot, (0, 18, Hold)),
    track!(SHIN_B, Rot, (0, -78, Hold)),
    track!(ARM_A, Rot, (0, 24, Hold)),
    track!(FOREARM_A, Rot, (0, -66, Hold)),
    track!(ARM_B, Rot, (0, 16, Hold)),
    track!(FOREARM_B, Rot, (0, -74, Hold)),
    track!(EAR_A, Rot, (0, 110, Hold)),
    track!(EAR_B, Rot, (0, 95, Hold)),
    track!(TAIL, Rot, (0, 250, Hold)),
];

static DOG_JUMP: Clip = Clip {
    species: &DOG,
    duration_ms: JUMP_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: 0,
    spin: Spin::None,
    tracks: DOG_JUMP_TRACKS,
};

static DOG_JUMP_FORWARD: Clip = Clip {
    species: &DOG,
    duration_ms: JUMP_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: TRAVEL_DX,
    spin: Spin::None,
    tracks: DOG_JUMP_TRACKS,
};

static DOG_CROUCH: Clip = Clip {
    species: &DOG,
    duration_ms: 1,
    loop_mode: LoopMode::Once,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[
        DOG_HIP_CROUCH,
        DOG_SPINE_80,
        DOG_NECK_148,
        DOG_HEAD_128,
        track!(THIGH_A, Rot, (0, 36, Hold)),
        track!(SHIN_A, Rot, (0, -78, Hold)),
        track!(THIGH_B, Rot, (0, 22, Hold)),
        track!(SHIN_B, Rot, (0, -88, Hold)),
        track!(ARM_A, Rot, (0, 32, Hold)),
        track!(FOREARM_A, Rot, (0, -74, Hold)),
        track!(ARM_B, Rot, (0, 18, Hold)),
        track!(FOREARM_B, Rot, (0, -82, Hold)),
    ],
};

static DOG_CRAWL: Clip = Clip {
    species: &DOG,
    duration_ms: WALK_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: CRAWL_DX,
    spin: Spin::None,
    tracks: &[
        DOG_HIP_CROUCH,
        DOG_SPINE_80,
        DOG_NECK_148,
        DOG_HEAD_128,
        track!(
            THIGH_A,
            Rot,
            (0, 22, Lerp),
            (278, 40, Lerp),
            (556, 50, Lerp),
            (833, 40, Lerp)
        ),
        track!(
            SHIN_A,
            Rot,
            (0, -88, Lerp),
            (278, -80, Lerp),
            (556, -72, Lerp),
            (833, -80, Lerp)
        ),
        track!(
            THIGH_B,
            Rot,
            (0, 50, Lerp),
            (278, 40, Lerp),
            (556, 22, Lerp),
            (833, 40, Lerp)
        ),
        track!(
            SHIN_B,
            Rot,
            (0, -72, Lerp),
            (278, -80, Lerp),
            (556, -88, Lerp),
            (833, -80, Lerp)
        ),
        track!(
            ARM_A,
            Rot,
            (0, 48, Lerp),
            (278, 36, Lerp),
            (556, 20, Lerp),
            (833, 36, Lerp)
        ),
        track!(
            FOREARM_A,
            Rot,
            (0, -70, Lerp),
            (278, -76, Lerp),
            (556, -82, Lerp),
            (833, -76, Lerp)
        ),
        track!(
            ARM_B,
            Rot,
            (0, 20, Lerp),
            (278, 36, Lerp),
            (556, 48, Lerp),
            (833, 36, Lerp)
        ),
        track!(
            FOREARM_B,
            Rot,
            (0, -82, Lerp),
            (278, -76, Lerp),
            (556, -70, Lerp),
            (833, -76, Lerp)
        ),
    ],
};

static DOG_BEG: Clip = Clip {
    species: &DOG,
    duration_ms: 1,
    loop_mode: LoopMode::Once,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[
        DOG_HIP_CROUCH,
        DOG_SPINE_140,
        DOG_NECK_155,
        DOG_HEAD_135,
        track!(THIGH_A, Rot, (0, 40, Hold)),
        track!(SHIN_A, Rot, (0, -86, Hold)),
        track!(THIGH_B, Rot, (0, 28, Hold)),
        track!(SHIN_B, Rot, (0, -96, Hold)),
        track!(ARM_A, Rot, (0, 70, Hold)),
        track!(FOREARM_A, Rot, (0, 120, Hold)),
        track!(ARM_B, Rot, (0, 62, Hold)),
        track!(FOREARM_B, Rot, (0, 118, Hold)),
    ],
};

static DOG_KNOCKBACK: Clip = Clip {
    species: &DOG,
    duration_ms: WALK_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: TRAVEL_DX,
    spin: Spin::Knockback,
    tracks: &[
        track!(THIGH_A, Rot, (0, 40, Hold)),
        track!(SHIN_A, Rot, (0, -40, Hold)),
        track!(THIGH_B, Rot, (0, -28, Hold)),
        track!(SHIN_B, Rot, (0, -90, Hold)),
        track!(ARM_A, Rot, (0, 50, Hold)),
        track!(FOREARM_A, Rot, (0, 70, Hold)),
        track!(ARM_B, Rot, (0, -40, Hold)),
        track!(FOREARM_B, Rot, (0, -30, Hold)),
        track!(ROOT, Spin, (0, 0, Lerp), (WALK_MS, 360, Lerp)),
    ],
};

static DOG_TUMBLE: Clip = Clip {
    species: &DOG,
    duration_ms: WALK_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: TRAVEL_DX,
    spin: Spin::Tumble,
    tracks: &[
        DOG_HIP_CROUCH,
        DOG_SPINE_90,
        track!(THIGH_A, Rot, (0, 50, Hold)),
        track!(SHIN_A, Rot, (0, -20, Hold)),
        track!(THIGH_B, Rot, (0, -20, Hold)),
        track!(SHIN_B, Rot, (0, -100, Hold)),
        track!(ARM_A, Rot, (0, 44, Hold)),
        track!(FOREARM_A, Rot, (0, 70, Hold)),
        track!(ARM_B, Rot, (0, 30, Hold)),
        track!(FOREARM_B, Rot, (0, 60, Hold)),
        track!(ROOT, Spin, (0, 0, Lerp), (WALK_MS, 360, Lerp)),
    ],
};

static DOG_FLIP: Clip = Clip {
    species: &DOG,
    duration_ms: FLIP_MS,
    loop_mode: LoopMode::Loop,
    travel_dx: 0,
    spin: Spin::None,
    tracks: &[track!(ROOT, Tx, (0, 0, Hold))],
};

/// Look up clip data. Searching reuses the crouch pose.
pub fn clip(id: ClipId) -> &'static Clip {
    match id {
        ClipId::Walk => &WALK,
        ClipId::Idle => &IDLE,
        ClipId::Jump => &JUMP,
        ClipId::JumpForward => &JUMP_FORWARD,
        ClipId::Crouch => &CROUCH,
        ClipId::Crawl => &CRAWL,
        ClipId::Beg => &BEG,
        ClipId::SwordStance => &SWORD_STANCE,
        ClipId::SwordStab => &SWORD_STAB,
        ClipId::SwordCrouchStance => &SWORD_CROUCH_STANCE,
        ClipId::SwordCrouchStab => &SWORD_CROUCH_STAB,
        ClipId::Knockback => &KNOCKBACK,
        ClipId::Tumble => &TUMBLE,
        ClipId::Flip => &FLIP,
        ClipId::BoxIdle => &BOX_IDLE,
        ClipId::BoxSlide => &BOX_SLIDE,
        ClipId::BoxRoll => &BOX_ROLL,
        ClipId::BoxShudder => &BOX_SHUDDER,
        ClipId::DogWalk => &DOG_WALK,
        ClipId::DogIdle => &DOG_IDLE,
        ClipId::DogJump => &DOG_JUMP,
        ClipId::DogJumpForward => &DOG_JUMP_FORWARD,
        ClipId::DogCrouch => &DOG_CROUCH,
        ClipId::DogCrawl => &DOG_CRAWL,
        ClipId::DogBeg => &DOG_BEG,
        ClipId::DogKnockback => &DOG_KNOCKBACK,
        ClipId::DogTumble => &DOG_TUMBLE,
        ClipId::DogFlip => &DOG_FLIP,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_travel_is_shared() {
        assert_eq!(WALK.travel_dx, TRAVEL_DX);
        assert_eq!(TUMBLE.travel_dx, TRAVEL_DX);
        assert_eq!(JUMP_FORWARD.travel_dx, TRAVEL_DX);
        assert_eq!(KNOCKBACK.travel_dx, TRAVEL_DX);
        assert_eq!(JUMP.travel_dx, 0);
        assert_eq!(CRAWL.travel_dx, CRAWL_DX);
        assert!(CRAWL.travel_dx < TRAVEL_DX);
        assert_eq!(CRAWL.loop_mode, LoopMode::Loop);
        assert_eq!(CRAWL.duration_ms, WALK_MS);
    }

    #[test]
    fn box_motion_clips_loop() {
        assert_eq!(BOX_SLIDE.loop_mode, LoopMode::Loop);
        assert_eq!(BOX_ROLL.loop_mode, LoopMode::Loop);
        assert_eq!(BOX_SHUDDER.loop_mode, LoopMode::Loop);
        assert!(BOX_SLIDE.travel_dx > 0);
        assert!(BOX_ROLL.travel_dx > 0);
        assert_eq!(BOX_ROLL.spin, Spin::Tumble);
        assert_eq!(BOX_IDLE.loop_mode, LoopMode::Once);
    }

    #[test]
    fn stickman_fits_scratch() {
        assert!(STICKMAN.bones.len() <= crate::stickman::ir::MAX_BONES);
        assert_eq!(STICKMAN.bones.len(), 16);
    }

    #[test]
    fn box_is_half_standing_height() {
        assert!(BOX.bones.len() <= crate::stickman::ir::MAX_BONES);
        assert_eq!(BOX_HEIGHT, (STANDING_HEIGHT / 2) as u32);
        assert_eq!(BOX_WIDTH, BOX_HEIGHT);
        assert_eq!(
            BOX.bones[0].kind,
            BoneKind::Rect {
                width: BOX_WIDTH,
                height: BOX_HEIGHT,
            }
        );
    }

    #[test]
    fn dog_is_a_compact_quadruped() {
        assert!(DOG.bones.len() <= crate::stickman::ir::MAX_BONES);
        assert_eq!(DOG.bones.len(), 16);
        assert_eq!(DOG.bones[SPINE as usize].length, DOG_SPINE);
        assert!(
            DOG_SPINE * 3 <= 36 * 2,
            "spine should be ~33% shorter than 36"
        );
        assert!(matches!(
            DOG.bones[HEAD as usize].kind,
            BoneKind::Ellipse { .. }
        ));
        assert!(matches!(
            DOG.bones[EAR_A as usize].kind,
            BoneKind::Triangle { .. }
        ));
        assert_eq!(DOG_WALK.species as *const _, &DOG as *const _);
        assert_eq!(DOG_IDLE.loop_mode, LoopMode::Once);
        assert_eq!(DOG_WALK.travel_dx, TRAVEL_DX);
        assert_eq!(DOG_JUMP.travel_dx, 0);
        assert_eq!(DOG_JUMP_FORWARD.travel_dx, TRAVEL_DX);
        assert_eq!(DOG_CRAWL.travel_dx, CRAWL_DX);
    }
}
