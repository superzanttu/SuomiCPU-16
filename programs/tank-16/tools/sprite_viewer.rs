//! Supplies sprite-atlas geometry for Tank-16's viewer.
//! Computes padded atlas dimensions, sprite IDs, and cell bounds, with geometry tests.

pub const SPRITE_SIZE: u32 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteBounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl AtlasSize {
    pub fn padded(self) -> Self {
        Self {
            width: self.width.div_ceil(SPRITE_SIZE) * SPRITE_SIZE,
            height: self.height.div_ceil(SPRITE_SIZE) * SPRITE_SIZE,
        }
    }

    pub fn columns(self) -> u32 {
        self.width.div_ceil(SPRITE_SIZE)
    }

    pub fn rows(self) -> u32 {
        self.height.div_ceil(SPRITE_SIZE)
    }

    pub fn sprite_id_at(self, x: u32, y: u32) -> Option<u32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some((y / SPRITE_SIZE) * self.columns() + x / SPRITE_SIZE)
    }

    pub fn sprite_bounds(self, sprite_id: u32) -> Option<SpriteBounds> {
        if sprite_id >= self.columns().checked_mul(self.rows())? {
            return None;
        }
        Some(SpriteBounds {
            x: sprite_id % self.columns() * SPRITE_SIZE,
            y: sprite_id / self.columns() * SPRITE_SIZE,
            width: SPRITE_SIZE,
            height: SPRITE_SIZE,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_coordinates_to_row_major_sprite_ids_and_bounds() {
        let atlas = AtlasSize {
            width: 192,
            height: 128,
        };
        assert_eq!(atlas.sprite_id_at(0, 0), Some(0));
        assert_eq!(atlas.sprite_id_at(191, 63), Some(2));
        assert_eq!(atlas.sprite_id_at(64, 64), Some(4));
        assert_eq!(
            atlas.sprite_bounds(4),
            Some(SpriteBounds {
                x: 64,
                y: 64,
                width: 64,
                height: 64
            })
        );
    }

    #[test]
    fn rounds_partial_atlas_cells_up_and_rejects_out_of_bounds() {
        let atlas = AtlasSize {
            width: 70,
            height: 65,
        };
        assert_eq!(
            atlas.padded(),
            AtlasSize {
                width: 128,
                height: 128
            }
        );
        assert_eq!(atlas.sprite_id_at(69, 64), Some(3));
        assert_eq!(atlas.sprite_bounds(3).unwrap().x, 64);
        assert_eq!(atlas.sprite_bounds(3).unwrap().y, 64);
        assert_eq!(atlas.sprite_id_at(70, 0), None);
        assert_eq!(atlas.sprite_bounds(4), None);
    }

    #[test]
    fn tank16_atlases_keep_their_expected_cell_layouts() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for (relative_path, expected) in [
            (
                "programs/tank-16/assets/gfx/tiles_objects.png",
                AtlasSize {
                    width: 384,
                    height: 512,
                },
            ),
            (
                "programs/tank-16/assets/gfx/tiles_terrain.png",
                AtlasSize {
                    width: 640,
                    height: 640,
                },
            ),
        ] {
            let bytes = std::fs::read(root.join(relative_path)).unwrap();
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
            assert_eq!(
                AtlasSize {
                    width: u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
                    height: u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
                },
                expected
            );
        }
    }
}
