//! dim2 线段收集器：rapier `DebugRenderBackend` → gfx `Geometry`。
//!
//! 设计文档 §2 换轨点：绘制端 v1 用 gfx 积木（`ShapeVertex{pos3,color4}`
//! + 索引绘制），本类型是数据面与绘制面的唯一交界面——若触发换轨条件
//! （单帧顶点逼近 u16 上限 / 需 pos2 精简），仅替换本模块实现，接口不变。
//!
//! ⚠️ 色彩空间：rapier `DebugColor` = **HSLA**——经 `crate::hsla` 转换。

use rapier2d::pipeline::{DebugRenderBackend, DebugRenderObject};
use rapier2d::prelude::{DebugColor, Vector};
use starfish_gfx::geometry::Geometry;

use crate::hsla::hsla_to_rgba;

/// 线段收集器：把 rapier debug 输出的线段流收进可复用的 [`Geometry`]。
///
/// - 缓冲复用：[`Self::clear`] 只清内容保留 capacity（每帧收集零分配）；
/// - u16 上限守卫：顶点逼近 `u16::MAX` 时丢弃后续线段并一次性告警
///   （设计文档 §2 换轨条件监测——常态场景不该触顶，触顶即该换轨）。
pub struct LineSink {
    geometry: Geometry,
    overflow_warned: bool,
}

impl LineSink {
    /// 空收集器。
    pub fn new() -> Self {
        Self {
            geometry: Geometry::default(),
            overflow_warned: false,
        }
    }

    /// 清空内容（保留 capacity——下一帧收集零分配）。
    pub fn clear(&mut self) {
        self.geometry.vertices.clear();
        self.geometry.indices.clear();
        self.overflow_warned = false;
    }

    /// 是否无线段。
    pub fn is_empty(&self) -> bool {
        self.geometry.indices.is_empty()
    }

    /// 顶点数（2 顶点/线段；u16 索引上限 = 65536 顶点 ≈ 3.2 万线段）。
    pub fn vertex_count(&self) -> usize {
        self.geometry.vertices.len()
    }

    /// 线段数。
    pub fn line_count(&self) -> usize {
        self.geometry.indices.len() / 2
    }

    /// 收集结果（RGBA 顶点 + u16 索引，LineList 索引对）。
    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    /// 灌入一条线段（内部与守卫逻辑的唯一入口）。
    fn push_line(&mut self, a: Vector, b: Vector, color: DebugColor) {
        // u16 索引上限守卫：超限丢弃（一次性告警）——设计文档 §2 换轨信号
        if self.geometry.vertices.len() + 2 > u16::MAX as usize {
            if !self.overflow_warned {
                self.overflow_warned = true;
                starfish_std::debug::console_log(
                    "[rapier-starfish-debug] LineSink 顶点逼近 u16 上限，后续线段丢弃——\
                     触发设计文档 §2 换轨条件（render 直绘无索引路线）",
                );
            }
            return;
        }
        let rgba = hsla_to_rgba(color);
        // 2D：z = 0 占位（ShapeVertex.pos3 的第三位）
        let ia = self.geometry.push([a.x, a.y, 0.0], rgba);
        let ib = self.geometry.push([b.x, b.y, 0.0], rgba);
        self.geometry.indices.push(ia);
        self.geometry.indices.push(ib);
    }
}

impl Default for LineSink {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugRenderBackend for LineSink {
    fn draw_line(&mut self, _object: DebugRenderObject, a: Vector, b: Vector, color: DebugColor) {
        self.push_line(a, b, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier2d::pipeline::DebugRenderObject;
    use rapier2d::prelude::*;

    /// 测试用 DebugRenderObject：standalone Collider + 假句柄
    /// （sink 的 draw_line 忽略 object，仅类型占位）
    struct FakeCollider {
        collider: Collider,
        handle: ColliderHandle,
    }
    impl FakeCollider {
        fn new() -> Self {
            Self {
                collider: ColliderBuilder::ball(0.5).build(),
                handle: ColliderHandle::from_raw_parts(0, 0),
            }
        }
        fn object(&self) -> DebugRenderObject<'_> {
            DebugRenderObject::Collider(self.handle, &self.collider)
        }
    }

    #[test]
    fn 收集_计数与缓冲复用() {
        let fake = FakeCollider::new();
        let mut sink = LineSink::new();
        let color = [120.0, 1.0, 0.5, 1.0];
        for i in 0..3 {
            sink.draw_line(
                fake.object(),
                Vector::new(i as f32, 0.0),
                Vector::new(i as f32, 1.0),
                color,
            );
        }
        assert_eq!(sink.line_count(), 3);
        assert_eq!(sink.vertex_count(), 6);
        assert_eq!(sink.geometry().indices.len(), 6);
        assert!(!sink.is_empty());

        // clear 后复用：计数归零、capacity 保留（零分配路径）
        let cap = sink.geometry().vertices.capacity();
        assert!(cap >= 6);
        sink.clear();
        assert!(sink.is_empty());
        for i in 0..3 {
            sink.draw_line(
                fake.object(),
                Vector::new(i as f32, 0.0),
                Vector::new(i as f32, 1.0),
                color,
            );
        }
        assert_eq!(sink.line_count(), 3);
        assert_eq!(sink.geometry().vertices.capacity(), cap);
    }

    #[test]
    fn u16上限守卫_超限丢弃不panic() {
        let fake = FakeCollider::new();
        let mut sink = LineSink::new();
        // 灌过守卫阈值（65536 顶点 = 32768 线段）
        let color = [0.0, 0.0, 0.0, 1.0];
        for _ in 0..34000 {
            sink.draw_line(
                fake.object(),
                Vector::new(0.0, 0.0),
                Vector::new(1.0, 0.0),
                color,
            );
        }
        assert!(sink.vertex_count() <= u16::MAX as usize);
        assert_eq!(sink.vertex_count() % 2, 0); // 恒整线段（2 顶点对齐）
        assert_eq!(sink.line_count(), u16::MAX as usize / 2);
    }

    #[test]
    fn 颜色经hsla转换落顶点() {
        let fake = FakeCollider::new();
        let mut sink = LineSink::new();
        sink.draw_line(
            fake.object(),
            Vector::new(0.0, 0.0),
            Vector::new(1.0, 0.0),
            [0.0, 1.0, 0.5, 1.0], // HSLA 红
        );
        let v = &sink.geometry().vertices[0];
        assert!((v.color[0] - 1.0).abs() < 1e-4);
        assert!(v.color[1].abs() < 1e-4 && v.color[2].abs() < 1e-4);
        assert!(v.pos[2].abs() < 1e-6); // 2D：z 恒 0
    }
}
