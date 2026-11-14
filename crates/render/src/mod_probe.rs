//! What the mod host's render probes change in the frame: the hurt tint of `RendererLivingEntity.setBrightness` and the
//! color of the enchantment glint of `RenderItem.renderEffect`. `None` keeps the vanilla value.

use crate::entity::{ARMOR_GLINT_COLOR, ITEM_GLINT_COLOR};
use crate::mesh::MeshData;

/// `setBrightness` for a hit entity: red at 0.3.
pub const VANILLA_HURT: [f32; 4] = [1.0, 0.0, 0.0, 0.3];

/// The armor glint is the item glint at 0.76 (`LayerArmorBase.renderGlint` multiplies the color by 0.5 over 0.65).
const ARMOR_SCALE: f32 = ARMOR_GLINT_COLOR[0] / ITEM_GLINT_COLOR[0];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    /// Red, green, blue and alpha of the tint that a hit entity is mixed with.
    pub hurt: Option<[f32; 4]>,
    /// Red, green, blue and alpha of the item glint.
    pub glint: Option<[f32; 4]>,
}

impl Look {
    pub fn hurt_tint(&self) -> [f32; 4] {
        self.hurt.unwrap_or(VANILLA_HURT)
    }

    /// Gives the vanilla glint colors of a glint layer the mod's color. A vertex that has another color is left alone.
    pub fn recolor_glint(&self, mesh: &mut MeshData) {
        let Some([r, g, b, a]) = self.glint else {
            return;
        };
        for v in &mut mesh.vertices {
            if v.color == ITEM_GLINT_COLOR {
                v.color = [r, g, b, a];
            } else if v.color == ARMOR_GLINT_COLOR {
                v.color = [r * ARMOR_SCALE, g * ARMOR_SCALE, b * ARMOR_SCALE, a];
            }
        }
    }

    pub fn item_glint(&self) -> [f32; 4] {
        self.glint.unwrap_or(ITEM_GLINT_COLOR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Vertex;

    fn vertex(color: [f32; 4]) -> Vertex {
        Vertex {
            color,
            ..bytemuck::Zeroable::zeroed()
        }
    }

    #[test]
    fn mod_color_replaces_both_vanilla_glint_colors() {
        let mut m = MeshData {
            vertices: vec![
                vertex(ITEM_GLINT_COLOR),
                vertex(ARMOR_GLINT_COLOR),
                vertex([1.0; 4]),
            ],
            indices: vec![],
        };
        Look {
            glint: Some([0.0, 1.0, 0.0, 1.0]),
            ..Look::default()
        }
        .recolor_glint(&mut m);
        assert_eq!(m.vertices[0].color, [0.0, 1.0, 0.0, 1.0]);
        let g = m.vertices[1].color;
        assert!((g[1] - ARMOR_SCALE).abs() < 1e-6 && g[0] == 0.0);
        assert_eq!(m.vertices[2].color, [1.0; 4]);
    }

    #[test]
    fn no_mod_means_vanilla() {
        let l = Look::default();
        assert_eq!(l.hurt_tint(), VANILLA_HURT);
        assert_eq!(l.item_glint(), ITEM_GLINT_COLOR);
    }
}
