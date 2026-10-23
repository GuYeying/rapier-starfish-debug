//! HSLA → RGBA 色彩转换（双维度共享）。
//!
//! rapier `DebugColor` = **HSLA**（`[h 0..=360, s, l, a]`），gfx 顶点色 =
//! RGBA——dim2/dim3 的收集器在入顶点前经 [`hsla_to_rgba`] 转换
//! （rapier 未提供助手；标准六扇区公式）。

/// rapier `DebugColor`（HSLA）→ RGBA（gfx 顶点色空间）。
///
/// 负 hue 经 `rem_euclid` 归一，s = 0 按灰度直通。
pub fn hsla_to_rgba(hsla: [f32; 4]) -> [f32; 4] {
    let [h, s, l, a] = hsla;
    if s <= 0.0 {
        let l = l.clamp(0.0, 1.0);
        return [l, l, l, a];
    }
    let h = h.rem_euclid(360.0) / 60.0; // 0..6 扇区
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s; // 色度
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0), // 0°~60°    红→黄
        1 => (x, c, 0.0), // 60°~120°  黄→绿
        2 => (0.0, c, x), // 120°~180° 绿→青
        3 => (0.0, x, c), // 180°~240° 青→蓝
        4 => (x, 0.0, c), // 240°~300° 蓝→品红
        _ => (c, 0.0, x), // 300°~360° 品红→红
    };
    [r + m, g + m, b + m, a]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsla转rgba_六扇区基值() {
        let eps = 1e-4;
        for (hsla, expected) in [
            ([0.0, 1.0, 0.5, 1.0], [1.0, 0.0, 0.0, 1.0]),
            ([120.0, 1.0, 0.5, 1.0], [0.0, 1.0, 0.0, 1.0]),
            ([240.0, 1.0, 0.5, 1.0], [0.0, 0.0, 1.0, 1.0]),
            ([60.0, 1.0, 0.5, 1.0], [1.0, 1.0, 0.0, 1.0]),
            ([180.0, 1.0, 0.5, 1.0], [0.0, 1.0, 1.0, 1.0]),
            ([300.0, 1.0, 0.5, 1.0], [1.0, 0.0, 1.0, 1.0]),
        ] {
            let got = hsla_to_rgba(hsla);
            for i in 0..4 {
                assert!(
                    (got[i] - expected[i]).abs() < eps,
                    "hsla {hsla:?} → {got:?}，期望 {expected:?}"
                );
            }
        }
    }

    #[test]
    fn hsla转rgba_灰度负hue与alpha直通() {
        // s = 0 → 灰度 = 亮度（hue 无效）
        let gray = hsla_to_rgba([999.0, 0.0, 0.25, 0.7]);
        assert!((gray[0] - 0.25).abs() < 1e-6 && (gray[2] - 0.25).abs() < 1e-6);
        assert!((gray[3] - 0.7).abs() < 1e-6);
        // 负 hue 归一（-90° ≡ 270°）
        let neg = hsla_to_rgba([-90.0, 1.0, 0.5, 1.0]);
        let pos = hsla_to_rgba([270.0, 1.0, 0.5, 1.0]);
        for i in 0..4 {
            assert!((neg[i] - pos[i]).abs() < 1e-5);
        }
    }
}
