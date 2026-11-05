//! What naga's SPIR-V backend (`third_party/naga`) writes differently from upstream for shader
//! packs, read back from the words it writes.

use std::collections::HashMap;

use naga::ShaderStage;
use naga::back::spv;
use naga::front::glsl::{Frontend, Options};
use naga::valid::{Capabilities, ValidationFlags, Validator};

const OP_EXT_INST: u32 = 12;
const OP_CONSTANT: u32 = 43;
const OP_CONSTANT_COMPOSITE: u32 = 44;
const GLSL_STD_450_SMOOTH_STEP: u32 = 49;

fn spirv(stage: ShaderStage, source: &str) -> Vec<u32> {
    let module = Frontend::default()
        .parse(&Options::from(stage), source)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap();
    spv::write_vec(&module, &info, &spv::Options::default(), None).unwrap()
}

/// Each instruction after the header: its opcode and operands.
fn instructions(words: &[u32]) -> Vec<(u32, &[u32])> {
    let mut out = Vec::new();
    let mut i = 5;
    while i < words.len() {
        let count = (words[i] >> 16) as usize;
        out.push((words[i] & 0xffff, &words[i + 1..i + count]));
        i += count;
    }
    out
}

/// The edges of each `SmoothStep`, constants of 32-bit floats (a vector by its first component).
fn smoothstep_edges(words: &[u32]) -> Vec<(f32, f32)> {
    let instructions = instructions(words);
    let mut constants = HashMap::new();
    for &(op, operands) in &instructions {
        match op {
            OP_CONSTANT => {
                constants.insert(operands[1], f32::from_bits(operands[2]));
            }
            OP_CONSTANT_COMPOSITE => {
                if let Some(&first) = constants.get(&operands[2]) {
                    constants.insert(operands[1], first);
                }
            }
            _ => {}
        }
    }
    instructions
        .iter()
        .filter(|&&(op, operands)| op == OP_EXT_INST && operands[3] == GLSL_STD_450_SMOOTH_STEP)
        .map(|&(_, operands)| (constants[&operands[4]], constants[&operands[5]]))
        .collect()
}

/// GLSL's `smoothstep`, as it is defined for edges either way round.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A smoothstep with constant edges the other way round, which GLSL.std.450 leaves undefined,
/// is written as one minus the rising one (the same values); one with rising edges as it is.
#[test]
fn falling_smoothstep_is_one_minus_the_rising_one() {
    let words = spirv(
        ShaderStage::Fragment,
        "#version 450\nlayout(location = 0) in vec3 v;\nlayout(location = 0) out vec4 color;\n\
         void main() {\n\
             float falling = smoothstep(0.04, 0.01, v.x);\n\
             float rising = smoothstep(0.01, 0.04, v.y);\n\
             vec3 falling3 = smoothstep(vec3(0.5), vec3(0.1), v);\n\
             color = vec4(falling3 * falling, rising);\n\
         }\n",
    );
    let mut edges = smoothstep_edges(&words);
    edges.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(edges, [(0.01, 0.04), (0.01, 0.04), (0.1, 0.5)]);
    for i in 0..=100 {
        let x = i as f32 * 0.0006;
        let falling = smoothstep(0.04, 0.01, x);
        assert!(
            (falling - (1.0 - smoothstep(0.01, 0.04, x))).abs() < 1e-6,
            "{x}"
        );
    }
}
