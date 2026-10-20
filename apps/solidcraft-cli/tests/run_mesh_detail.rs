//! `run --out` writes mesh files at `--mesh-detail`, `--mesh-tolerance` and `--mesh-angle`;
//! the flags need a mesh output and valid values.

use std::path::PathBuf;
use std::process::Command;

use serde_json::json;

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("solidcraft-run-mesh-detail-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }

    /// A Ø10 × 10 cylinder.
    fn script(&self) -> PathBuf {
        let path = self.0.join("pin.json");
        std::fs::write(&path, json!({"commands": [{"command": "solid.cylinder", "params": {"radius": 5, "height": 10}}]}).to_string()).unwrap();
        path
    }

    fn file(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_solidcraft-cli"))
}

/// Triangles of a binary STL.
fn triangles(path: &str) -> u32 {
    let b = std::fs::read(path).unwrap();
    u32::from_le_bytes(b[80..84].try_into().unwrap())
}

#[test]
fn mesh_detail_flags_set_the_triangulation() {
    let s = Scratch::new("flags");
    let script = s.script();
    let run = |out: &str, extra: &[&str]| {
        let o = cli().arg("run").arg(&script).args(["--out", out]).args(extra).arg("--quiet").output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    };
    let (coarse, fine, custom) = (s.file("c.stl"), s.file("f.stl"), s.file("x.stl"));
    run(&coarse, &["--mesh-detail", "coarse"]);
    run(&fine, &["--mesh-detail", "fine"]);
    run(&custom, &["--mesh-tolerance", "0.001", "--mesh-angle", "2"]);
    assert!(triangles(&coarse) < triangles(&fine));
    assert!(triangles(&fine) < triangles(&custom));
    // A STEP beside a mesh output is written as before; the flags apply to the mesh.
    let (stl, step) = (s.file("a.stl"), s.file("a.step"));
    let o = cli().arg("run").arg(&script).args(["--out", &stl, "--out", &step, "--mesh-detail", "fine"]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(triangles(&stl), triangles(&fine));
}

#[test]
fn mesh_detail_flags_refuse_what_they_cannot_do() {
    let s = Scratch::new("refuse");
    let script = s.script();
    let fails = |args: &[&str], says: &str| {
        let o = cli().arg("run").arg(&script).args(args).output().unwrap();
        let err = String::from_utf8_lossy(&o.stderr).to_string();
        assert!(!o.status.success() && err.contains(says), "{args:?}: {err}");
    };
    fails(&["--out", &s.file("a.step"), "--mesh-detail", "fine"], "need a mesh --out");
    fails(&["--out", &s.file("a.stl"), "--mesh-detail", "ultra"], "coarse, standard or fine");
    fails(&["--out", &s.file("a.stl"), "--mesh-angle", "90"], "angle");
    fails(&["--out", &s.file("a.stl"), "--mesh-tolerance", "fine"], "needs a number");
    assert!(!std::path::Path::new(&s.file("a.stl")).exists());
}
