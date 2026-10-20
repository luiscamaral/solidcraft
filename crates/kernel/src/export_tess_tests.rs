//! Meshes for files (`Body::tessellate_export`): the chord tolerance alone is the reference
//! tessellation, and a facet angle keeps a small radius round on a large body.

use std::collections::HashSet;
use std::f64::consts::TAU;

use solidcraft_geom::{Mesh, Vec3};

use super::Body;
use crate::cylinder;

/// Distinct points of `m` on the circle of radius `r` about the Z axis at height `z`.
fn rim_points(m: &Mesh, r: f64, z: f64) -> usize {
    let on: HashSet<(i64, i64)> = m
        .positions
        .iter()
        .filter(|p| (p.z - z).abs() < 1e-6 && ((p.x * p.x + p.y * p.y).sqrt() - r).abs() < 1e-6)
        .map(|p| ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64))
        .collect();
    on.len()
}

/// Largest distance from a wall triangle of `m` (all its corners on the cylinder, not all at one
/// height: cap triangles can have three rim corners too) to the cylinder's surface: the sagitta
/// at its centroid.
fn side_deviation(m: &Mesh, r: f64) -> f64 {
    let radial = |p: Vec3| (p.x * p.x + p.y * p.y).sqrt();
    m.triangles
        .iter()
        .filter_map(|t| m.tri(t))
        .filter(|[a, b, c]| [a, b, c].iter().all(|p| (radial(**p) - r).abs() < 1e-6))
        .filter(|[a, b, c]| a.z.max(b.z).max(c.z) - a.z.min(b.z).min(c.z) > 1e-6)
        .map(|[a, b, c]| r - radial((a + b + c) / 3.0))
        .fold(0.0, f64::max)
}

fn pin() -> Body {
    cylinder(Vec3::ZERO, Vec3::Z, 1.5, 10.0).unwrap()
}

#[test]
fn export_without_angle_is_the_reference_tessellation() {
    let b = pin();
    for tol in [0.2, 0.01] {
        let (a, e) = (b.tessellate(tol).unwrap(), b.tessellate_export(tol, None).unwrap());
        let bits = |m: &Mesh| m.positions.iter().map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]).collect::<Vec<_>>();
        assert_eq!(bits(&a), bits(&e));
        assert_eq!(a.triangles, e.triangles);
    }
}

/// A Ø3 mm pin at the chord a 1 m part exports with (0.2 mm): the tolerance alone leaves a few
/// sides; a 10 degree facet angle gives at least 36, each within the tolerance.
#[test]
fn facet_angle_keeps_a_small_radius_round() {
    let b = pin();
    let coarse = b.tessellate_export(0.2, None).unwrap();
    let round = b.tessellate_export(0.2, Some(10f64.to_radians())).unwrap();
    let (n_coarse, n_round) = (rim_points(&coarse, 1.5, 10.0), rim_points(&round, 1.5, 10.0));
    assert!(n_coarse < 36, "{n_coarse}");
    assert!(n_round >= (TAU / 10f64.to_radians()).ceil() as usize, "{n_round}");
    // No facet turns more than the angle: the widest gap between neighbouring rim points.
    let mut at: Vec<f64> = round
        .positions
        .iter()
        .filter(|p| (p.z - 10.0).abs() < 1e-6 && ((p.x * p.x + p.y * p.y).sqrt() - 1.5).abs() < 1e-6)
        .map(|p| p.y.atan2(p.x).rem_euclid(TAU))
        .collect();
    at.sort_by(f64::total_cmp);
    at.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let widest = at.windows(2).map(|w| w[1] - w[0]).chain(at.first().zip(at.last()).map(|(f, l)| f + TAU - l)).fold(0.0, f64::max);
    assert!(widest <= 10f64.to_radians() + 1e-9, "{} degrees", widest.to_degrees());
    assert!(side_deviation(&round, 1.5) <= 0.2);
}

#[test]
fn chord_tolerance_bounds_the_deviation() {
    let b = pin();
    for tol in [0.1, 0.01, 0.001] {
        let m = b.tessellate_export(tol, None).unwrap();
        let dev = side_deviation(&m, 1.5);
        assert!(dev <= tol * (1.0 + 1e-9), "tol {tol}: {dev}");
    }
}

#[test]
fn hostile_tolerances_and_angles_never_panic() {
    let b = pin();
    for tol in [f64::NAN, -1.0, 0.0, f64::INFINITY, 1e-300, 1e300] {
        for angle in [None, Some(f64::NAN), Some(0.0), Some(-1.0), Some(7.0), Some(f64::INFINITY), Some(1e-300)] {
            if let Ok(m) = b.tessellate_export(tol, angle) {
                assert!(m.positions.iter().all(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite()));
            }
        }
    }
}
