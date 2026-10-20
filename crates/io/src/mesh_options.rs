//! How finely solids are triangulated when they are written as meshes (STL, OBJ, 3MF, the
//! meshes of a URDF package): a preset detail, or an explicit chord tolerance and facet angle.
//! The default writes what earlier versions wrote.

/// Preset detail of an exported mesh.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MeshDetail {
    /// Small files: 1/1000 of the body's size (at least 5 µm), no facet angle. Never more
    /// triangles than [`MeshDetail::Standard`]; small holes on large parts keep few sides.
    Coarse,
    /// What earlier versions wrote: 1/5000 of the body's size (at least 1 µm), no facet angle.
    #[default]
    Standard,
    /// For printing: 1/20000 of the body's size, never coarser than 0.01 mm (at least 1 µm),
    /// facets of at most 10°, so small holes and rounds stay round on large parts.
    Fine,
}

impl MeshDetail {
    /// From its name (`coarse`, `standard`, `fine`; any case).
    pub fn from_name(name: &str) -> Option<MeshDetail> {
        match name.trim().to_ascii_lowercase().as_str() {
            "coarse" => Some(MeshDetail::Coarse),
            "standard" => Some(MeshDetail::Standard),
            "fine" => Some(MeshDetail::Fine),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            MeshDetail::Coarse => "coarse",
            MeshDetail::Standard => "standard",
            MeshDetail::Fine => "fine",
        }
    }
}

/// Explicit chord tolerances accepted, in millimetres.
pub const MESH_TOLERANCE_MM: std::ops::RangeInclusive<f64> = 0.0005..=10.0;
/// Explicit facet angles accepted, in degrees.
pub const MESH_ANGLE_DEG: std::ops::RangeInclusive<f64> = 1.0..=45.0;
/// Most triangles one export writes (a binary STL of this many is 1 GB).
pub const MAX_EXPORT_TRIANGLES: usize = 20_000_000;

/// Options for the meshes written to files. The default writes what earlier versions wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshOptions {
    pub detail: MeshDetail,
    /// Chord tolerance in mm (the largest distance from a triangle to the surface), instead of
    /// the preset's.
    pub tolerance: Option<f64>,
    /// Facet angle in radians (the most a facet turns across a curved face), instead of the
    /// preset's.
    pub angle: Option<f64>,
}

impl MeshOptions {
    /// The chord tolerance (mm) and facet angle (radians, `None`: no limit) for a body whose
    /// characteristic size (bounding-box diagonal) is `size` mm.
    pub fn resolve(&self, size: f64) -> (f64, Option<f64>) {
        let size = if size.is_finite() && size > 0.0 { size } else { 1.0 };
        let (chord, angle) = match self.detail {
            MeshDetail::Coarse => ((size * 1e-3).max(0.005), None),
            MeshDetail::Standard => ((size * 2e-4).max(1e-3), None),
            MeshDetail::Fine => ((size * 5e-5).clamp(1e-3, 0.01), Some(10f64.to_radians())),
        };
        let tolerance = self.tolerance.filter(|t| t.is_finite() && *t > 0.0).unwrap_or(chord);
        let angle = self.angle.filter(|a| a.is_finite() && *a > 0.0).or(angle);
        (tolerance, angle)
    }
}

/// What an export wrote for one body.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshInfo {
    pub body: String,
    /// Triangles written for the body.
    pub triangles: usize,
    /// Chord tolerance (mm) and facet angle (radians) it was meshed with; `None` for a mesh
    /// body, which is written as it is.
    pub tolerance: Option<f64>,
    pub angle: Option<f64>,
    /// Signed volume of the triangles written (mm³).
    pub volume: f64,
}

/// The bytes of an export and, for mesh formats, what was written per body.
#[derive(Clone, Debug, Default)]
pub struct Exported {
    pub bytes: Vec<u8>,
    pub meshes: Vec<MeshInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_is_the_earlier_export() {
        for size in [0.5, 1.0, 17.3, 424.3, 1001.3, 1e6] {
            assert_eq!(MeshOptions::default().resolve(size), ((size * 2e-4).max(1e-3), None));
        }
    }

    #[test]
    fn fine_is_never_coarser_than_a_hundredth_of_a_millimetre() {
        let fine = MeshOptions { detail: MeshDetail::Fine, ..MeshOptions::default() };
        for size in [0.01, 1.0, 17.3, 424.3, 1001.3, 1e6] {
            let (t, a) = fine.resolve(size);
            assert!((1e-3..=0.01).contains(&t), "{size}: {t}");
            assert_eq!(a, Some(10f64.to_radians()));
        }
        // Coarse is looser than Standard without a facet angle, Fine tighter with one: the
        // presets never swap order on any body.
        let coarse = MeshOptions { detail: MeshDetail::Coarse, ..MeshOptions::default() };
        for size in [0.01, 1.0, 4.9, 5.0, 17.3, 424.3, 1001.3, 1e6] {
            let (c, s, f) = (coarse.resolve(size), MeshOptions::default().resolve(size), fine.resolve(size));
            assert!(c.0 >= s.0 && c.1.is_none(), "{size}");
            assert!(f.0 <= s.0 && f.1.is_some(), "{size}");
        }
    }

    #[test]
    fn explicit_values_win_and_hostile_sizes_resolve() {
        let o = MeshOptions { detail: MeshDetail::Coarse, tolerance: Some(0.02), angle: None };
        assert_eq!(o.resolve(100.0), (0.02, None));
        let o = MeshOptions { detail: MeshDetail::Standard, tolerance: None, angle: Some(0.1) };
        assert_eq!(o.resolve(100.0), (0.02, Some(0.1)));
        for size in [f64::NAN, -1.0, 0.0, f64::INFINITY] {
            let (t, _) = MeshOptions::default().resolve(size);
            assert!(t.is_finite() && t > 0.0);
        }
        assert_eq!(MeshDetail::from_name(" FINE "), Some(MeshDetail::Fine));
        assert_eq!(MeshDetail::from_name("ultra"), None);
        for d in [MeshDetail::Coarse, MeshDetail::Standard, MeshDetail::Fine] {
            assert_eq!(MeshDetail::from_name(d.name()), Some(d));
        }
    }
}
