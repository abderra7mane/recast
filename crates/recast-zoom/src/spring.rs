/// One exact step of a critically damped spring pulling `x` toward `target`.
/// Exact integration keeps it stable for any `dt` and free of overshoot.
pub fn step(x: f64, v: f64, target: f64, omega: f64, dt: f64) -> (f64, f64) {
    let c1 = x - target;
    let c2 = v + omega * c1;
    let e = (-omega * dt).exp();
    let offset = (c1 + c2 * dt) * e;
    (target + offset, (c2 - omega * (c1 + c2 * dt)) * e)
}

/// 0 at `t <= 0`, 1 at `t >= 1`, smooth in between.
pub fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_without_overshoot() {
        let (mut x, mut v) = (0.0, 0.0);
        let mut previous = x;
        for _ in 0..2_000 {
            (x, v) = step(x, v, 1.0, 8.0, 0.001);
            assert!(x >= previous - 1e-12 && x <= 1.0 + 1e-12);
            previous = x;
        }
        assert!((x - 1.0).abs() < 1e-4, "{x}");
    }

    #[test]
    fn step_size_does_not_change_the_path() {
        let (mut fine, mut fv) = (0.0, 0.0);
        for _ in 0..100 {
            (fine, fv) = step(fine, fv, 1.0, 7.0, 0.001);
        }
        let (coarse, _) = step(0.0, 0.0, 1.0, 7.0, 0.1);
        assert!((fine - coarse).abs() < 1e-9);
    }

    #[test]
    fn smoothstep_edges() {
        assert_eq!(smoothstep(-1.0), 0.0);
        assert_eq!(smoothstep(0.5), 0.5);
        assert_eq!(smoothstep(2.0), 1.0);
    }
}
