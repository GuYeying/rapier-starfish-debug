//! dim3 debug 渲染数据面（设计文档 §5/§7）：物理状态 → 线段流。
//!
//! 控制面 = rapier [`DebugRenderPipeline`] **直透**（与 dim2 完全镜像；
//! `style`/`mode` 均为 pub 字段零包装）。⚠️ [`DebugDraw`] 须**持久持有**
//! （pipeline 构造含细分轮廓缓存分配）。

pub mod render;
pub mod sink;

pub use render::DebugLinesRenderer;
pub use sink::LineSink;

use rapier3d::pipeline::{DebugRenderMode, DebugRenderPipeline, DebugRenderStyle};

use crate::dim3::world::PhysicsWorld;

/// debug 绘制状态（rapier pipeline 的持久持有壳；直透控制面）。
pub struct DebugDraw {
    pipeline: DebugRenderPipeline,
}

impl DebugDraw {
    /// 按渲染内容位创建（style = rapier 默认）。
    pub fn new(mode: DebugRenderMode) -> Self {
        Self::with_style(mode, DebugRenderStyle::default())
    }

    /// 按渲染内容位 + 渲染类型创建。
    pub fn with_style(mode: DebugRenderMode, style: DebugRenderStyle) -> Self {
        Self {
            pipeline: DebugRenderPipeline::new(style, mode),
        }
    }

    /// 控制面直通（`style` / `mode` 均可运行期直改）。
    pub fn pipeline(&mut self) -> &mut DebugRenderPipeline {
        &mut self.pipeline
    }
}

impl Default for DebugDraw {
    fn default() -> Self {
        Self::new(DebugRenderMode::COLLIDER_SHAPES | DebugRenderMode::CONTACTS)
    }
}

/// 物理状态 → 线段流（每帧调用；`sink` 内部 clear + 复用缓冲）。
pub fn collect(world: &PhysicsWorld, draw: &mut DebugDraw, sink: &mut LineSink) {
    sink.clear();
    world.get_ref().debug_render(&mut draw.pipeline, sink);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim3::world::PhysicsWorld;
    use rapier3d::prelude::*;

    /// 地面（固定）+ 动球的最小场景
    fn sample_world() -> PhysicsWorld {
        let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
        world.get().insert(
            RigidBodyBuilder::fixed(),
            ColliderBuilder::cuboid(5.0, 0.5, 5.0),
        );
        world.get().insert(
            RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 3.0, 0.0)),
            ColliderBuilder::ball(0.5),
        );
        world
    }

    #[test]
    fn 空世界_空输出() {
        let world = PhysicsWorld::new([0.0, -9.81, 0.0]);
        let mut draw = DebugDraw::default();
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        assert!(sink.is_empty());
    }

    #[test]
    fn 空mode位_有体也无输出() {
        let world = sample_world();
        let mut draw = DebugDraw::new(DebugRenderMode::empty());
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        assert!(sink.is_empty());
    }

    #[test]
    fn 形状模式_盒十二边对账() {
        // 仅固定地面（3D cuboid = 盒轮廓 = 12 边）
        let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
        world.get().insert(
            RigidBodyBuilder::fixed(),
            ColliderBuilder::cuboid(5.0, 0.5, 5.0),
        );
        let mut draw = DebugDraw::new(DebugRenderMode::COLLIDER_SHAPES);
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        assert_eq!(sink.line_count(), 12, "3D 盒轮廓 = 12 线段");
        assert_eq!(sink.vertex_count(), 24);
    }

    #[test]
    fn 球细分度随style() {
        // 仅动球：3D 球轮廓 = 3 个正交大圆 × subdivisions
        for sub in [8u32, 16] {
            let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
            world.get().insert(
                RigidBodyBuilder::dynamic(),
                ColliderBuilder::ball(0.5),
            );
            let style = DebugRenderStyle {
                subdivisions: sub,
                ..DebugRenderStyle::default()
            };
            let mut draw = DebugDraw::with_style(DebugRenderMode::COLLIDER_SHAPES, style);
            let mut sink = LineSink::new();
            collect(&world, &mut draw, &mut sink);
            assert_eq!(sink.line_count(), sub as usize * 3, "subdivisions = {sub}");
        }
    }

    #[test]
    fn mode位叠加_输出单调增() {
        let world = sample_world();
        let mut sink = LineSink::new();
        let mut count = |mode| {
            let mut draw = DebugDraw::new(mode);
            collect(&world, &mut draw, &mut sink);
            sink.line_count()
        };
        let shapes = count(DebugRenderMode::COLLIDER_SHAPES);
        let with_aabb = count(DebugRenderMode::COLLIDER_SHAPES | DebugRenderMode::COLLIDER_AABBS);
        assert!(shapes > 0);
        assert!(with_aabb > shapes, "叠加 AABB 位输出应增多");
    }

    #[test]
    fn 配色直透_style改色落顶点() {
        let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
        world.get().insert(
            RigidBodyBuilder::fixed(),
            ColliderBuilder::cuboid(5.0, 0.5, 5.0),
        );
        let style = DebugRenderStyle {
            collider_fixed_color: [0.0, 1.0, 0.5, 1.0],
            ..DebugRenderStyle::default()
        };
        let mut draw = DebugDraw::with_style(DebugRenderMode::COLLIDER_SHAPES, style);
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        assert!(sink.vertex_count() > 0);
        for v in sink.geometry().vertices.iter() {
            assert!(
                (v.color[0] - 1.0).abs() < 1e-4 && v.color[1].abs() < 1e-4,
                "固定体线色应为 RGBA 红，实际 {:?}",
                v.color
            );
        }
    }

    #[test]
    fn 睡眠乘数直透_休眠体变暗() {
        let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
        let (rb, _) = world.get().insert(
            RigidBodyBuilder::dynamic(),
            ColliderBuilder::ball(0.5),
        );
        world.get().bodies[rb].sleep(); // 强制休眠
        let style = DebugRenderStyle {
            sleep_color_multiplier: [1.0, 1.0, 0.2, 1.0],
            ..DebugRenderStyle::default()
        };
        let mut draw = DebugDraw::with_style(DebugRenderMode::COLLIDER_SHAPES, style);
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        assert!(sink.vertex_count() > 0, "休眠体仍应产生线段（只是变暗）");
        for v in sink.geometry().vertices.iter() {
            let lum = 0.2126 * v.color[0] + 0.7152 * v.color[1] + 0.0722 * v.color[2];
            assert!(lum < 0.4, "休眠体线应为暗色（乘数生效），亮度 {lum}");
        }
        draw.pipeline().style.sleep_color_multiplier = [1.0; 4];
        collect(&world, &mut draw, &mut sink);
        assert!(sink.vertex_count() > 0);
    }

    #[test]
    fn 复用收集_跨帧capacity稳定() {
        let mut world = sample_world();
        let mut draw = DebugDraw::default();
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        let cap = sink.geometry().vertices.capacity();
        let lines = sink.line_count();
        world.step();
        collect(&world, &mut draw, &mut sink);
        assert_eq!(sink.geometry().vertices.capacity(), cap);
        assert!(sink.line_count() >= lines - 1 && sink.line_count() <= lines + 4);
    }
}
