//! Optional wgpu forward hook (feature `wgpu`, off by default).
//!
//! The CPU pipeline in [`crate::pipeline`] needs no GPU to run or test.
//! This module only describes how resolved [`crate::spike::SpikeSample`] data
//! reaches a renderer: one instance matrix slot per spike, matching the three
//! `InstancedMesh` draws (one per crystal variant) of the original
//! `IceAbility`. A full `wgpu` renderer backend is follow-up work, not part
//! of this crate's default build.

/// Instance attributes for a spike slot: a column-major 3x3 basis + translation
/// packed as four `float32x4`s (offsets 0/16/32/48), bound from shader
/// location 4 upward so locations 0–3 stay free for crystal vertex data.
pub fn spike_instance_attributes() -> [wgpu::VertexAttribute; 4] {
    const ATTRS: [wgpu::VertexAttribute; 4] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 0,
            shader_location: 4,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 16,
            shader_location: 5,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 32,
            shader_location: 6,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 48,
            shader_location: 7,
        },
    ];
    ATTRS
}

/// Stride in bytes of one spike instance slot (4 × `vec4<f32>`).
pub const SPIKE_INSTANCE_STRIDE: u64 = 64;

/// How many instanced draws cover the whole field (one per crystal variant,
/// mirroring `VARIANTS = 3` in `IceAbility.js`).
pub const CRYSTAL_VARIANT_PASSES: u32 = 3;
