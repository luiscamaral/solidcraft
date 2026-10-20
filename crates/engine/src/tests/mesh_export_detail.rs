//! Mesh export detail: `file.export` and `file.save_mesh` take `detail`, `tolerance`, `angle`
//! and `report`. No options write the mesh earlier versions wrote; Fine keeps a small hole round
//! on a large part; exact formats refuse the options; hostile values never panic. Expected
//! values are analytic.

use std::collections::HashSet;

use serde_json::{Value, json};

use super::run;
use crate::*;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("solidcraft-mesh-detail-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn path(dir: &std::path::Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// A 1000 × 50 × 10 mm bar with a Ø3 mm through hole at (500, 25): its size (1 m) gives the
/// standard export a 0.2 mm chord.
fn bar() -> Session {
    let mut s = Session::default();
    run(&mut s, "solid.box", json!({"length": 1000, "width": 50, "height": 10, "corner": [0, 0, 0]}));
    run(&mut s, "solid.hole", json!({"position": [500, 25, 10], "diameter": 3}));
    s
}

/// The triangles of a binary STL.
fn stl_triangles(bytes: &[u8]) -> Vec<[[f32; 3]; 3]> {
    let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    (0..n)
        .map(|i| {
            let at = |k: usize| {
                let o = 84 + i * 50 + 12 + k * 12;
                let f = |j: usize| f32::from_le_bytes(bytes[o + j * 4..o + j * 4 + 4].try_into().unwrap());
                [f(0), f(1), f(2)]
            };
            [at(0), at(1), at(2)]
        })
        .collect()
}

/// Sides of the Ø3 hole: distinct rim points on the bar's top face.
fn hole_sides(bytes: &[u8]) -> usize {
    let rim: HashSet<(i64, i64)> = stl_triangles(bytes)
        .iter()
        .flatten()
        .filter(|p| (p[2] - 10.0).abs() < 1e-3 && ((p[0] as f64 - 500.0).hypot(p[1] as f64 - 25.0) - 1.5).abs() < 0.01)
        .map(|p| ((p[0] as f64 * 1e3).round() as i64, (p[1] as f64 * 1e3).round() as i64))
        .collect();
    rim.len()
}

#[test]
fn fine_keeps_a_small_hole_round_on_a_large_part() {
    let dir = temp_dir("hole");
    let mut s = bar();
    let standard = run(&mut s, "file.export", json!({"path": path(&dir, "standard.stl")}));
    let fine = run(&mut s, "file.export", json!({"path": path(&dir, "fine.stl"), "detail": "fine"}));
    let (bs, bf) = (std::fs::read(dir.join("standard.stl")).unwrap(), std::fs::read(dir.join("fine.stl")).unwrap());
    let (ns, nf) = (hole_sides(&bs), hole_sides(&bf));
    // Standard: the 0.2 mm chord of a 1 m part leaves the hole an octagon (2.77 mm across flats).
    assert!(ns <= 8, "{ns}");
    // Fine: at most 10 degrees per facet, so at least 36 sides; across the flats within 0.01 mm.
    assert!(nf >= 36, "{nf}");
    assert!(3.0 - 3.0 * (std::f64::consts::PI / nf as f64).cos() <= 0.01);
    assert_eq!(standard["detail"], "standard");
    assert_eq!(fine["detail"], "fine");
    assert!(fine["triangles"].as_u64().unwrap() > standard["triangles"].as_u64().unwrap());
    let m = &fine["meshes"][0];
    assert_eq!(m["angle"].as_f64(), Some(10.0));
    assert!(m["tolerance"].as_f64().unwrap() <= 0.01);
    assert_eq!(standard["meshes"][0]["angle"], Value::Null);
    assert!((standard["meshes"][0]["tolerance"].as_f64().unwrap() - 1001.3 * 2e-4).abs() < 1e-3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn explicit_tolerance_and_angle_override_the_preset() {
    let dir = temp_dir("explicit");
    let mut s = bar();
    let r = run(&mut s, "file.export", json!({"path": path(&dir, "a.3mf"), "detail": "coarse", "tolerance": 0.05, "angle": 20}));
    assert_eq!(r["meshes"][0]["tolerance"].as_f64(), Some(0.05));
    assert!((r["meshes"][0]["angle"].as_f64().unwrap() - 20.0).abs() < 1e-9);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_mesh_and_export_write_the_same_mesh() {
    let dir = temp_dir("agree");
    let mut s = bar();
    for (detail, angle) in [("fine", Value::Null), ("coarse", json!(15))] {
        run(&mut s, "file.export", json!({"path": path(&dir, "e.stl"), "detail": detail, "angle": angle}));
        run(&mut s, "file.save_mesh", json!({"path": path(&dir, "m.stl"), "detail": detail, "angle": angle}));
        // Byte 80 on: the header names the file.
        assert_eq!(std::fs::read(dir.join("e.stl")).unwrap()[80..], std::fs::read(dir.join("m.stl")).unwrap()[80..], "{detail}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exact_formats_refuse_mesh_options() {
    let dir = temp_dir("exact");
    let mut s = bar();
    for (file, key, value) in
        [("a.step", "detail", json!("fine")), ("a.igs", "tolerance", json!(0.01)), ("a.json", "angle", json!(10)), ("b.step", "report", json!(true))]
    {
        let e = s.execute("file.export", &json!({"path": path(&dir, file), key: value})).unwrap_err().to_string();
        assert!(e.contains("mesh formats"), "{file}: {e}");
        assert!(!dir.join(file).exists(), "{file} written");
    }
    // Without them the exact formats export as before.
    run(&mut s, "file.export", json!({"path": path(&dir, "ok.step")}));
    let _ = std::fs::remove_dir_all(&dir);
}

/// `report: true` measures the solid's volume as Measure does (within about 1e-4 of the analytic
/// π r² h) and the mesh's deviation from it, which shrinks with detail.
#[test]
fn report_measures_the_volume_deviation() {
    let dir = temp_dir("report");
    let mut s = Session::default();
    run(&mut s, "solid.cylinder", json!({"radius": 5, "height": 10}));
    let exact = std::f64::consts::PI * 25.0 * 10.0;
    let mut last = f64::INFINITY;
    for detail in ["coarse", "standard", "fine"] {
        let r = run(&mut s, "file.export", json!({"path": path(&dir, "c.stl"), "detail": detail, "report": true}));
        let m = &r["meshes"][0];
        let (v, ev, dev) = (m["volume"].as_f64().unwrap(), m["solid_volume"].as_f64().unwrap(), m["volume_deviation"].as_f64().unwrap());
        assert!((ev - exact).abs() / exact < 2e-4, "{ev}");
        assert!((dev - (v - ev) / ev).abs() < 1e-12);
        assert!(dev < 0.0 && dev.abs() <= last, "{detail}: {dev}");
        last = dev.abs();
    }
    // Without `report`, the solid is not measured.
    let r = run(&mut s, "file.export", json!({"path": path(&dir, "c.stl"), "detail": "fine"}));
    assert!(r["meshes"][0].get("solid_volume").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn urdf_meshes_take_the_detail() {
    let dir = temp_dir("urdf");
    let mut s = Session::default();
    run(&mut s, "solid.cylinder", json!({"radius": 5, "height": 10}));
    let size = |s: &mut Session, detail: &str| {
        let r = run(s, "file.export", json!({"path": path(&dir, "r.urdf"), "detail": detail}));
        r["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .filter(|f| f.ends_with(".stl"))
            .map(|f| std::fs::metadata(f).unwrap().len())
            .sum::<u64>()
    };
    assert!(size(&mut s, "coarse") < size(&mut s, "fine"));
    let e = s.execute("file.export", &json!({"path": path(&dir, "r.urdf"), "report": true})).unwrap_err().to_string();
    assert!(e.contains("report"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `file.export` and `file.save_mesh` are left out of `hostile_params_never_panic` (they write
/// files), so their mesh options get hostile values here: every call answers, a refused one
/// writes nothing, and the design and its undo history are unchanged.
#[test]
fn hostile_mesh_options_never_panic() {
    let dir = temp_dir("hostile");
    let mut s = bar();
    let before = s.doc.to_json();
    let long = "x".repeat(10_000);
    let hostile = [
        json!(null),
        json!(f64::MAX),
        json!(-1),
        json!(0),
        json!(1e-300),
        json!(1e300),
        json!(i64::MIN),
        json!(u64::MAX),
        json!("NaN"),
        json!("inf"),
        json!("-0.01"),
        json!(long),
        json!(true),
        json!([1, 2, 3]),
        json!({"a": {"b": [1]}}),
        json!([[[[[[[[[[1]]]]]]]]]]),
        json!("fine\u{0}"),
    ];
    let mut k = 0;
    for cmd in ["file.export", "file.save_mesh"] {
        for key in ["detail", "tolerance", "angle", "report"] {
            for v in &hostile {
                k += 1;
                let file = format!("h{k}.stl");
                let p = json!({"path": path(&dir, &file), key: v});
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.execute(cmd, &p)));
                let Ok(r) = r else { panic!("{cmd} {key}={v} panicked") };
                if r.is_err() {
                    assert!(!dir.join(&file).exists(), "{cmd} {key}={v}: refused but written");
                }
            }
        }
    }
    assert_eq!(s.doc.to_json(), before, "an export changed the design");
    run(&mut s, "file.export", json!({"path": path(&dir, "after.stl"), "detail": "fine"}));
    let _ = std::fs::remove_dir_all(&dir);
}
