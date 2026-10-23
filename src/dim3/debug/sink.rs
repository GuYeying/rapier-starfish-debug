//! dim3 线段收集器：rapier `DebugRenderBackend` → gfx `Geometry`。
//!
//! 设计文档 §2 换轨点（与 dim2 镜像）；3D 差异仅一处：`draw_line` 的
//! `pos3` **直通** `ShapeVertex.pos3`（零转换）。色彩空间：HSLA →
//! `crate::hsla` 转换。

use rapier3d::pipeline::{DebugRenderBackend, DebugRenderObject};
use rapier3d::prelude::{DebugColor, Vector};
use starfish_gfx::geometry::Geometry;

use crate::hsla::hsla_to_rgba;

/// 线段收集器（3D；缓冲复用 + u16 上限守卫，语义见 dim2 同名文档）。
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

    /// 顶点数（u16 索引上限 = 65536 顶点）。
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
        if self.geometry.vertices.len() + 2 > u16::MAX as usize {
            if !self.overflow_warned {
                self.overflow_warned = true;
                starfish_std::debug::console_log(
                    "[rapier-starfish-debug] LineSink(3D) 顶点逼近 u16 上限，后续线段丢弃——\
                     触发设计文档 §2 换轨条件（render 直绘无索引路线）",
                );
            }
            return;
        }
        let rgba = hsla_to_rgba(color);
        // 3D：pos3 直通（零转换）
        let ia = self.geometry.push([a.x, a.y, a.z], rgba);
        let ib = self.geometry.push([b.x, b.y, b.z], rgba);
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
    use rapier3d::pipeline::DebugRenderObject;
    use rapier3d::prelude::*;

    /// 测试用 DebugRenderObject：standalone Collider + 假句柄
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
                Vector::new(i as f32, 0.0, 0.0),
                Vector::new(i as f32, 1.0, 0.0),
                color,
            );
        }
        assert_eq!(sink.line_count(), 3);
        assert_eq!(sink.vertex_count(), 6);
        assert_eq!(sink.geometry().indices.len(), 6);
        assert!(!sink.is_empty());

        let cap = sink.geometry().vertices.capacity();
        assert!(cap >= 6);
        sink.clear();
        assert!(sink.is_empty());
        for i in 0..3 {
            sink.draw_line(
                fake.object(),
                Vector::new(i as f32, 0.0, 0.0),
                Vector::new(i as f32, 1.0, 0.0),
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
        let color = [0.0, 0.0, 0.0, 1.0];
        for _ in 0..34000 {
            sink.draw_line(
                fake.object(),
                Vector::new(0.0, 0.0, 0.0),
                Vector::new(1.0, 0.0, 0.0),
                color,
            );
        }
        assert!(sink.vertex_count() <= u16::MAX as usize);
        assert_eq!(sink.vertex_count() % 2, 0);
        assert_eq!(sink.line_count(), u16::MAX as usize / 2);
    }

    #[test]
    fn 颜色经hsla转换落顶点_z直通() {
        let fake = FakeCollider::new();
        let mut sink = LineSink::new();
        sink.draw_line(
            fake.object(),
            Vector::new(0.0, 0.0, 3.0),
            Vector::new(1.0, 0.0, 4.0),
            [0.0, 1.0, 0.5, 1.0], // HSLA 红
        );
        let v0 = &sink.geometry().vertices[0];
        assert!((v0.color[0] - 1.0).abs() < 1e-4);
        assert!(v0.color[1].abs() < 1e-4 && v0.color[2].abs() < 1e-4);
        // 3D：z 直通零转换
        assert!((v0.pos[2] - 3.0).abs() < 1e-6);
        let v1 = &sink.geometry().vertices[1];
        assert!((v1.pos[2] - 4.0).abs() < 1e-6);
    }
}
